//! USB access on Linux: the logged-in user may only open the machine when a udev rule grants it.
//! The `.deb` and `.rpm` packages install the rule; an AppImage or a build from source cannot, so
//! the app checks for one and offers to set it up (through `pkexec`, which asks for the password)
//! or shows the command to paste into a terminal.

use std::path::Path;

use crate::model::{ApiError, UsbAccess};

/// The rule the packages install, built into the app.
pub const RULES: &str = include_str!("../../../../packaging/linux/70-bladepilot.rules");

/// Where the app writes the rule.
pub const RULES_PATH: &str = "/etc/udev/rules.d/70-bladepilot.rules";

const RULE_DIRS: [&str; 3] = ["/etc/udev/rules.d", "/usr/lib/udev/rules.d", "/lib/udev/rules.d"];

/// Does any rule file mention Graphtec's vendor id (`0b4d`)? Then some package already grants
/// access; BladePilot's own or another driver's.
pub fn rule_present(dirs: &[&Path]) -> bool {
    dirs.iter().filter_map(|dir| std::fs::read_dir(dir).ok()).flatten().flatten().any(|entry| {
        let path = entry.path();
        path.extension().is_some_and(|e| e == "rules")
            && std::fs::read_to_string(&path).is_ok_and(|text| text.to_ascii_lowercase().contains("idvendor}==\"0b4d\""))
    })
}

/// The shell lines that write the rule and reload udev, without `sudo`.
fn script() -> String {
    let lines: Vec<String> = RULES.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).map(|l| format!("'{l}'")).collect();
    format!("printf '%s\\n' {} > {RULES_PATH} && udevadm control --reload && udevadm trigger --subsystem-match=usb", lines.join(" "))
}

/// The command to paste into a terminal.
pub fn command() -> String {
    let lines: Vec<String> = RULES.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).map(|l| format!("'{l}'")).collect();
    format!(
        "printf '%s\\n' {} | sudo tee {RULES_PATH} > /dev/null && sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=usb",
        lines.join(" ")
    )
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// How things stand with USB access.
pub fn status() -> UsbAccess {
    let needed = cfg!(target_os = "linux");
    let dirs: Vec<&Path> = RULE_DIRS.iter().map(Path::new).collect();
    UsbAccess {
        needed,
        installed: !needed || rule_present(&dirs),
        command: if needed { command() } else { String::new() },
        can_elevate: needed && on_path("pkexec"),
    }
}

/// Writes the rule with `pkexec`, which asks for the administrator password.
pub fn install() -> Result<(), ApiError> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    let output = std::process::Command::new("pkexec")
        .args(["sh", "-c", &script()])
        .output()
        .map_err(|e| ApiError::new("elevate", format!("pkexec could not be started: {e}")))?;
    match output.status.code() {
        Some(0) => Ok(()),
        // 126: the password dialog was dismissed; 127: not authorised.
        Some(126 | 127) => Err(ApiError::new("elevateDenied", "the password was not given")),
        _ => Err(ApiError::new("elevate", String::from_utf8_lossy(&output.stderr).trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_writes_every_rule_line() {
        let cmd = command();
        for line in RULES.lines().filter(|l| l.starts_with("SUBSYSTEM")) {
            assert!(cmd.contains(&format!("'{line}'")), "{line}");
        }
        assert!(cmd.contains("sudo tee /etc/udev/rules.d/70-bladepilot.rules"));
        assert!(!script().contains("sudo"));
    }

    #[test]
    fn a_rule_is_found_by_the_vendor_id() {
        let dir = std::env::temp_dir().join(format!("bladepilot-udev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!rule_present(&[dir.as_path()]));
        std::fs::write(dir.join("50-other.rules"), "SUBSYSTEM==\"usb\", ATTR{idVendor}==\"1234\"\n").unwrap();
        assert!(!rule_present(&[dir.as_path()]));
        std::fs::write(dir.join("70-bladepilot.rules"), RULES).unwrap();
        assert!(rule_present(&[dir.as_path(), Path::new("/does/not/exist")]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
