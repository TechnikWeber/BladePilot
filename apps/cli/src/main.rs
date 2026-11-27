//! `bladepilot`: the command-line front end.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use bladepilot_design::{Design, Rgb};
use bladepilot_device::{JobControl, Progress, RunOptions, Session, SimTransport, Transport, bluetooth, usb};
use bladepilot_job::{CutSettings, Grouping, JobSpec, Plan};
use bladepilot_machines::{Machine, Support};
use bladepilot_sim::{Config, VirtualPlotter};
use clap::{Args, Parser, Subcommand};
use craft_plotter::gpgl::silhouette::Mat;

#[derive(Parser)]
#[command(name = "bladepilot", version, about = "Cutting software for Silhouette Portrait and Cameo machines")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the machines BladePilot knows
    Machines,
    /// List cutting machines connected over USB, and with --bluetooth those in Bluetooth range
    Devices {
        /// Also search for machines over Bluetooth (about ten seconds)
        #[arg(long)]
        bluetooth: bool,
    },
    /// Show what a design file contains
    Inspect {
        /// SVG, .studio or .studio3 file
        file: PathBuf,
    },
    /// Plan a design, then cut it, draw it with the virtual plotter or print the program
    Cut(CutArgs),
    /// Cut a small test pattern (a 12 mm square with a triangle inside) to try settings
    TestCut(TestCutArgs),
    /// Read a USB capture (Wireshark pcap/pcapng with USBPcap or usbmon) and show what the machine got
    Replay(ReplayArgs),
}

#[derive(Args)]
struct ReplayArgs {
    /// The capture file
    capture: PathBuf,
    /// Machine the capture was made with, for drawing and checks
    #[arg(long, short, default_value = "portrait3")]
    machine: String,
    /// The machine as BUS:ADDRESS when the capture holds more than one
    #[arg(long)]
    device: Option<bladepilot_capture::Device>,
    /// Draw what the machine was told to cut into this SVG file
    #[arg(long, value_name = "SVG")]
    svg: Option<PathBuf>,
    /// Show every message, also each repeated status poll
    #[arg(long)]
    all: bool,
    /// Print only the commands, one per line, to compare two captures with diff
    #[arg(long, conflicts_with = "all")]
    commands_only: bool,
}

#[derive(Args)]
struct CutArgs {
    /// SVG, .studio or .studio3 file
    file: PathBuf,
    #[command(flatten)]
    target: MachineArgs,
    /// Where the page's top-left corner lands on the mat, in mm: X,Y
    #[arg(long, value_parser = parse_pair, default_value = "0,0", allow_hyphen_values = true)]
    offset: (f64, f64),
    /// One pass per colour instead of one pass for everything
    #[arg(long)]
    by_color: bool,
    /// With --by-color: shapes of this colour are drawn with a pen (repeatable), e.g. "#ff0000"
    #[arg(long, value_name = "HEX", requires = "by_color")]
    pen_color: Vec<String>,
    /// With --pen-color: force for the pen passes (default: the machine's own, not --force)
    #[arg(long, requires = "pen_color")]
    pen_force: Option<u32>,
    #[command(flatten)]
    tool: ToolArgs,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Args)]
struct TestCutArgs {
    /// Where the test square's top-left corner goes on the mat, in mm: X,Y
    #[arg(long, value_parser = parse_pair, default_value = "5,5", allow_hyphen_values = true)]
    at: (f64, f64),
    #[command(flatten)]
    target: MachineArgs,
    #[command(flatten)]
    tool: ToolArgs,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Args)]
struct MachineArgs {
    /// Machine id, see `bladepilot machines`
    #[arg(long, short, default_value = "portrait3")]
    machine: String,
    /// Mat: 8x12, 8.5x12, 12x12, 12x24, 15x15, 24x24 or none (default: the machine's usual mat)
    #[arg(long)]
    mat: Option<String>,
}

#[derive(Args)]
struct ToolArgs {
    /// Force in the machine's steps (default: the machine's setting)
    #[arg(long)]
    force: Option<u32>,
    /// Speed in the machine's steps (default: the machine's setting)
    #[arg(long)]
    speed: Option<u32>,
    /// AutoBlade depth 0-10
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=10))]
    depth: Option<u8>,
    /// How often every path is cut
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=10))]
    passes: u8,
    /// Draw with a pen instead of cutting with a blade
    #[arg(long)]
    pen: bool,
    /// Run the blade this far past sharp corners, mm (0-2)
    #[arg(long, default_value_t = 0.0)]
    overcut: f64,
    /// Blade swivel offset, mm
    #[arg(long, default_value_t = 0.9)]
    blade_offset: f64,
}

#[derive(Args)]
struct OutputArgs {
    /// Print the program instead of cutting
    #[arg(long, conflicts_with = "simulate")]
    dry_run: bool,
    /// With --dry-run: write the exact bytes to this file instead of printing commands
    #[arg(long, requires = "dry_run", value_name = "FILE")]
    bytes: Option<PathBuf>,
    /// Run the job on the virtual plotter and draw it into this SVG file
    #[arg(long, value_name = "SVG")]
    simulate: Option<PathBuf>,
    /// Send to a real machine over USB
    #[arg(long, conflicts_with_all = ["dry_run", "simulate"])]
    experimental_usb: bool,
    /// Send to a real machine over Bluetooth: its address (see `bladepilot devices --bluetooth`),
    /// or none to search for it
    #[arg(long, value_name = "ADDRESS", num_args = 0..=1, conflicts_with_all = ["dry_run", "simulate", "experimental_usb"])]
    experimental_bluetooth: Option<Option<String>>,
}

fn parse_pair(s: &str) -> Result<(f64, f64), String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y in mm, e.g. 10,15")?;
    let num = |v: &str| v.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or(format!("`{v}` is not a number"));
    Ok((num(x)?, num(y)?))
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        // `bladepilot machines | head` closes the pipe early: not an error.
        Err(e) if e.chain().any(|c| c.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)) => {
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Machines => machines(),
        Command::Devices { bluetooth } => devices(bluetooth),
        Command::Inspect { file } => inspect(&file),
        Command::Cut(args) => cut(&args),
        Command::TestCut(args) => test_cut(&args),
        Command::Replay(args) => replay(&args),
    }
}

fn machines() -> Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{:<16} {:<32} {:<10} {:>7}  {:<28} SUPPORT", "ID", "NAME", "USB", "WIDTH", "MATS")?;
    for m in bladepilot_machines::all() {
        let mats = m.mats.iter().map(|mat| mat.id()).collect::<Vec<_>>().join(", ");
        let support = match m.support {
            Support::Target => "target",
            Support::Untested => "untested",
        };
        writeln!(out, "{:<16} {:<32} {:04x}:{:04x} {:>4.0} mm  {:<28} {support}", m.id, m.name, m.vendor_id, m.product_id, m.cut_width_mm, mats)?;
    }
    Ok(())
}

fn devices(with_bluetooth: bool) -> Result<()> {
    let found = usb::scan().context("scanning USB")?;
    let mut out = std::io::stdout().lock();
    if found.is_empty() {
        writeln!(out, "No cutting machine found on USB.")?;
    }
    for f in found {
        let name = f.machine.map_or("unknown Graphtec device", |m| m.name);
        writeln!(
            out,
            "{name}  ({:04x}:{:04x}, bus {} address {}{})",
            f.vendor_id,
            f.product_id,
            f.bus,
            f.address,
            f.product.map(|p| format!(", reports \"{p}\"")).unwrap_or_default()
        )?;
    }
    if with_bluetooth {
        bluetooth_devices(&mut out)?;
    }
    Ok(())
}

/// Searches over Bluetooth and says how to send to what it finds, or what to check.
fn bluetooth_devices(out: &mut impl std::io::Write) -> Result<()> {
    eprintln!("searching Bluetooth for about ten seconds ...");
    let found = bluetooth::scan()?;
    if found.is_empty() {
        writeln!(out, "No cutting machine found over Bluetooth.")?;
        writeln!(out, "  - Is the machine switched on and its blue Bluetooth light on (blinking)? If it is off, press the Bluetooth button.")?;
        writeln!(out, "  - Is the machine still connected to Silhouette Studio or a phone? Close that connection first.")?;
        writeln!(out, "  - It needs no pairing and no PIN: BladePilot connects directly.")?;
        return Ok(());
    }
    for f in &found {
        let name = f.machine.map_or("unknown machine", |m| m.name);
        writeln!(out, "{name}  (Bluetooth {}, \"{}\")", f.address, f.name)?;
    }
    if let Some(f) = found.iter().find(|f| f.machine.is_some()) {
        let id = f.machine.map_or("portrait4", |m| m.id);
        writeln!(out, "Send to it with: bladepilot cut FILE --machine {id} --experimental-bluetooth {}", f.address)?;
        writeln!(out, "Connecting can take up to a minute when the machine was just disconnected.")?;
    }
    Ok(())
}

fn load(file: &Path) -> Result<Design> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let name = file.file_name().and_then(|n| n.to_str());
    bladepilot_design::import(&bytes, name).with_context(|| format!("opening {}", file.display()))
}

fn color_name(c: Option<Rgb>) -> String {
    c.map_or_else(|| "no colour".to_string(), Rgb::hex)
}

fn inspect(file: &Path) -> Result<()> {
    let design = load(file)?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}: {}", file.display(), design.format.name())?;
    if let Some((w, h)) = design.page_mm {
        writeln!(out, "page      {w:.1} × {h:.1} mm")?;
    }
    writeln!(out, "shapes    {}", design.shapes.len())?;
    if let Some(bb) = design.bbox() {
        writeln!(out, "extent    {:.1} × {:.1} mm, from ({:.1}, {:.1})", bb.width(), bb.height(), bb.x0, bb.y0)?;
    }
    for c in design.cut_colors() {
        let n = design.shapes.iter().filter(|s| s.cut_color() == c).count();
        writeln!(out, "colour    {:<10} {n} shape(s)", color_name(c))?;
    }
    for w in &design.warnings {
        writeln!(out, "note      {w}")?;
    }
    Ok(())
}

fn spec_for(target: &MachineArgs, tool: &ToolArgs) -> Result<JobSpec> {
    let machine: &'static Machine = bladepilot_machines::by_id(&target.machine)
        .with_context(|| format!("unknown machine `{}`; `bladepilot machines` lists them", target.machine))?;
    let mut spec = JobSpec::new(machine);
    if let Some(mat) = &target.mat {
        spec.mat = Mat::parse(mat).with_context(|| format!("unknown mat `{mat}`"))?;
    }
    spec.settings = CutSettings {
        force: tool.force,
        speed: tool.speed,
        depth: tool.depth,
        passes: tool.passes,
        pen: tool.pen,
        overcut_mm: tool.overcut,
        blade_offset_mm: tool.blade_offset,
    };
    Ok(spec)
}

fn print_plan(plan: &Plan) {
    let (down, right) = plan.area_mm;
    eprintln!("machine   {} on the {} mat (cutting area {right:.1} × {down:.1} mm)", plan.machine.name, plan.mat.id());
    for (i, pass) in plan.passes.iter().enumerate() {
        let s = &pass.settings;
        let fmt = |v: Option<u32>| v.map_or_else(|| "machine".to_string(), |v| v.to_string());
        eprintln!(
            "pass {}    {:<10} {} path(s), {}, force {}, speed {}, depth {}, {}×",
            i + 1,
            color_name(pass.color),
            pass.paths.len(),
            if s.pen { "pen" } else { "blade" },
            fmt(s.force),
            fmt(s.speed),
            s.depth.map_or_else(|| "machine".to_string(), |d| d.to_string()),
            s.passes
        );
    }
    eprintln!("length    {:.0} mm cut, {:.0} mm travel between cuts", plan.cut_length_mm(), plan.travel_length_mm());
}

fn cut(args: &CutArgs) -> Result<()> {
    let design = load(&args.file)?;
    for w in &design.warnings {
        eprintln!("note      {w}");
    }
    let mut spec = spec_for(&args.target, &args.tool)?;
    spec.offset_mm = args.offset;
    spec.grouping = if args.by_color { Grouping::ByColor } else { Grouping::Single };
    for hex in &args.pen_color {
        let color = Rgb::parse(hex).with_context(|| format!("`{hex}` is not a colour like #ff0000"))?;
        // A pen keeps the speed and passes, but not the blade's depth and force.
        spec.color_settings.insert(color, CutSettings { pen: true, depth: None, force: args.pen_force, ..spec.settings });
    }
    let plan = bladepilot_job::plan(&design, &spec)?;
    execute(&plan, &args.output)
}

fn test_cut(args: &TestCutArgs) -> Result<()> {
    let spec = spec_for(&args.target, &args.tool)?;
    let plan = bladepilot_job::plan(&bladepilot_job::test_pattern(args.at), &spec)?;
    eprintln!("test cut  12 mm square with a triangle inside at {:.1}, {:.1} mm", args.at.0, args.at.1);
    execute(&plan, &args.output)
}

/// Prints the plan, then prints the program, runs it on the virtual plotter or sends it.
fn execute(plan: &Plan, output: &OutputArgs) -> Result<()> {
    print_plan(plan);
    let program = bladepilot_job::encode(plan)?;

    if output.dry_run {
        match &output.bytes {
            Some(path) => {
                std::fs::write(path, program.to_stream()).with_context(|| format!("writing {}", path.display()))?;
                eprintln!("wrote     {} ({} bytes)", path.display(), program.to_stream().len());
            }
            None => {
                let mut out = std::io::stdout().lock();
                for line in program.transcript() {
                    writeln!(out, "{line}")?;
                }
            }
        }
        return Ok(());
    }

    if let Some(svg) = &output.simulate {
        let plotter = VirtualPlotter::new(Config::new(plan.machine));
        let mut session = Session::instant(SimTransport::new(plotter));
        session.run(&program, |_| {}).context("virtual plotter")?;
        let plotter = session.into_transport().plotter;
        std::fs::write(svg, plotter.to_svg()).with_context(|| format!("writing {}", svg.display()))?;
        let stats = plotter.stats();
        eprintln!(
            "simulated {} commands, {} cut(s), {:.0} mm cut, {:.0} mm travel with the final park move, {} warning(s)",
            stats.commands, stats.cuts, stats.cut_length_mm, stats.travel_length_mm, stats.warnings
        );
        for w in plotter.warnings() {
            eprintln!("warning   {}", w.message);
        }
        eprintln!("drawn     {}", svg.display());
        return Ok(());
    }

    if let Some(address) = &output.experimental_bluetooth {
        return send_bluetooth(plan, &program, address.as_deref());
    }
    if !output.experimental_usb {
        bail!("cutting on a real machine is still experimental: add --experimental-usb or --experimental-bluetooth (or use --dry-run or --simulate FILE.svg)");
    }
    send_usb(plan, &program)
}

fn tool_name(pen: bool) -> &'static str {
    if pen { "pen" } else { "blade" }
}

fn send_usb(plan: &Plan, program: &craft_plotter::Program) -> Result<()> {
    let machine = plan.machine;
    let found = usb::scan().context("scanning USB")?;
    let target =
        found.iter().find(|f| f.machine.is_some_and(|m| m.id == machine.id)).or_else(|| found.first()).context("no cutting machine found on USB")?;
    if target.machine.is_none_or(|m| m.id != machine.id) {
        bail!("the connected machine ({:04x}:{:04x}) is not a {}: pass the right --machine", target.vendor_id, target.product_id, machine.name);
    }
    let transport = usb::open(target).context("opening the machine")?;
    send(plan, program, transport)
}

fn send_bluetooth(plan: &Plan, program: &craft_plotter::Program, address: Option<&str>) -> Result<()> {
    let machine = plan.machine;
    let address = match address {
        Some(a) => a.parse::<bluetooth::Address>().map_err(anyhow::Error::msg)?,
        None => {
            eprintln!("searching Bluetooth for the {} (about ten seconds) ...", machine.name);
            let found = bluetooth::scan()?;
            let hit = found.iter().find(|f| f.machine.is_some_and(|m| m.id == machine.id)).map(|f| f.address);
            hit.with_context(|| format!("no {} found over Bluetooth: is its blue light on? (`bladepilot devices --bluetooth`)", machine.name))?
        }
    };
    eprintln!("connecting over Bluetooth to {address} (up to a minute) ...");
    let transport = bluetooth::open(address)?;
    send(plan, program, transport)
}

/// Runs the job on a real machine and reports on the terminal.
fn send(plan: &Plan, program: &craft_plotter::Program, transport: Box<dyn Transport + Send>) -> Result<()> {
    let mut session = Session::new(transport);
    session.init()?;
    eprintln!("connected {}", session.version().unwrap_or_else(|_| "(no version reply)".to_string()));
    // Every pass goes to holder 1: between a pen and a blade pass the tool has to be swapped.
    let options = RunOptions {
        pass_starts: bladepilot_job::pass_starts(plan, program),
        pause_before: plan.tool_changes(),
        pass_pens: plan.passes.iter().map(|p| p.settings.pen).collect(),
    };
    let control = JobControl::new();
    session.run_with(program, &options, &control, |p| match p {
        Progress::Ready => eprintln!("ready"),
        Progress::Pass { index } => {
            let pass = &plan.passes[index];
            eprintln!("\npass {}    {} with the {}", index + 1, color_name(pass.color), tool_name(pass.settings.pen));
        }
        Progress::Sending { sent, total } => {
            eprint!("\rsending   {:>3} %", sent * 100 / total.max(1))
        }
        Progress::SearchingMarks => eprintln!("\nsearching for the registration marks"),
        Progress::Paused { before_pass: Some(index) } => {
            let pen = plan.passes[index].settings.pen;
            eprint!("\nchange    pass {} needs the {}: put it into the holder, then press Enter ", index + 1, tool_name(pen));
            let _ = std::io::stdin().read_line(&mut String::new());
            control.resume();
        }
        Progress::Paused { before_pass: None } => control.resume(),
        Progress::WrongTool { before_pass } => {
            let pen = plan.passes[before_pass].settings.pen;
            eprint!("\nwrong tool pass {} needs the {}, but the machine reports another tool", before_pass + 1, tool_name(pen));
        }
        Progress::Resumed => {}
        Progress::Finishing => eprintln!("\nfinishing"),
        Progress::Done => eprintln!("done"),
    })?;
    Ok(())
}

fn replay(args: &ReplayArgs) -> Result<()> {
    use bladepilot_capture::{Message, fold_polls, printable};
    let machine: &'static Machine =
        bladepilot_machines::by_id(&args.machine).with_context(|| format!("unknown machine `{}`; `bladepilot machines` lists them", args.machine))?;
    let bytes = std::fs::read(&args.capture).with_context(|| format!("reading {}", args.capture.display()))?;
    let capture = bladepilot_capture::read(&bytes).with_context(|| format!("reading {}", args.capture.display()))?;
    eprintln!("capture   {}: {} packets, {} bulk transfers with data", args.capture.display(), capture.packets, capture.transfers.len());
    let device = match args.device {
        Some(device) => device,
        None => capture.machine().context("no device in the capture talks like a cutting machine; pick one with --device BUS:ADDRESS")?,
    };
    for t in capture.devices() {
        let mark = if t.device == device { "  <- machine" } else { "" };
        eprintln!("device    {:<7} {:>9} bytes to it, {:>7} bytes from it{mark}", t.device.to_string(), t.to_device, t.from_device);
    }
    let messages = capture.messages(device);
    let mut out = std::io::stdout().lock();
    if args.commands_only {
        for m in &messages {
            if let Message::Command(c) = m {
                writeln!(out, "{}", printable(c.as_bytes()))?;
            }
        }
    } else if args.all {
        for m in &messages {
            writeln!(out, "{}", m.describe())?;
        }
    } else {
        for (line, count) in fold_polls(&messages) {
            if count > 1 {
                writeln!(out, "{line}  (x{count})")?;
            } else {
                writeln!(out, "{line}")?;
            }
        }
    }
    let mut plotter = VirtualPlotter::new(Config::new(machine));
    plotter.write(&capture.stream_to(device));
    let stats = plotter.stats();
    eprintln!(
        "simulated {} commands, {} cut(s), {:.0} mm cut, {:.0} mm travel, {} warning(s)",
        stats.commands, stats.cuts, stats.cut_length_mm, stats.travel_length_mm, stats.warnings
    );
    for w in plotter.warnings() {
        eprintln!("warning   {}", w.message);
    }
    if let Some(svg) = &args.svg {
        std::fs::write(svg, plotter.to_svg()).with_context(|| format!("writing {}", svg.display()))?;
        eprintln!("drawn     {}", svg.display());
    }
    Ok(())
}
