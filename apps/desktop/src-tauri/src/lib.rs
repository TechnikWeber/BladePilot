//! BladePilot's desktop app. The commands below only hand their arguments to [`service`], which
//! does the work with the workspace crates; the web front end in `ui/` calls them.
//!
//! Everything slow runs off the main thread: imports, plans and simulations on Tauri's blocking
//! pool, a job on a thread of its own that reports through a channel. The window never waits for
//! the machine.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

pub mod model;
pub mod service;
pub mod udev;

use model::{
    ApiError, AppInfo, DesignInfo, DeviceInfo, DeviceStatus, JobAction, JobEvent, JobRequest, MachineInfo, MadeSpec, OpenedProject, PlacedItem,
    PlanInfo, ProjectRequest, Simulation, Target, TidyResult, TraceRequest, UsbAccess,
};
use service::Service;

type Shared<'a> = State<'a, Arc<Service>>;

/// Runs `work` on the blocking pool.
async fn off_main<T: Send + 'static>(work: impl FnOnce() -> Result<T, ApiError> + Send + 'static) -> Result<T, ApiError> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| ApiError::new("internal", e.to_string()))?
}

#[tauri::command]
fn app_info() -> AppInfo {
    service::app_info()
}

#[tauri::command]
async fn usb_access() -> Result<UsbAccess, ApiError> {
    off_main(|| Ok(udev::status())).await
}

#[tauri::command]
async fn install_usb_access() -> Result<UsbAccess, ApiError> {
    off_main(|| {
        udev::install()?;
        Ok(udev::status())
    })
    .await
}

#[tauri::command]
fn machines() -> Vec<MachineInfo> {
    service::machines()
}

/// Design files named on the command line (`bladepilot-desktop a.svg b.studio3`, or "Open with"
/// in a file manager), to open at start: once, not again when the page reloads.
#[tauri::command]
fn startup_files() -> Vec<PathBuf> {
    static TAKEN: AtomicBool = AtomicBool::new(false);
    if TAKEN.swap(true, Ordering::SeqCst) {
        return Vec::new();
    }
    service::startup_files(std::env::args_os().skip(1))
}

#[tauri::command]
async fn open_design(service: Shared<'_>, path: PathBuf) -> Result<DesignInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.open_design(&path)).await
}

#[tauri::command]
async fn import_design(service: Shared<'_>, name: String, bytes: Vec<u8>) -> Result<DesignInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.import_design(&name, &bytes)).await
}

#[tauri::command]
async fn trace_design(service: Shared<'_>, id: u32, trace: TraceRequest) -> Result<DesignInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.trace_design(id, &trace)).await
}

#[tauri::command]
async fn contour_path(service: Shared<'_>, id: u32, offset_mm: f64) -> Result<String, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.contour_path(id, offset_mm)).await
}

#[tauri::command]
async fn print_sheet(service: Shared<'_>, job: JobRequest, path: PathBuf) -> Result<(), ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.print_sheet(&job, &path)).await
}

#[tauri::command]
async fn fonts(service: Shared<'_>) -> Result<Vec<String>, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || Ok(service.fonts())).await
}

#[tauri::command]
async fn make_design(service: Shared<'_>, spec: MadeSpec) -> Result<DesignInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.make_design(&spec)).await
}

#[tauri::command]
async fn save_project(service: Shared<'_>, path: PathBuf, project: ProjectRequest) -> Result<(), ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.save_project(&path, &project)).await
}

#[tauri::command]
async fn open_project(service: Shared<'_>, path: PathBuf) -> Result<OpenedProject, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.open_project(&path)).await
}

/// Where the last session is kept between starts.
fn autosave_path(app: &AppHandle) -> Result<PathBuf, ApiError> {
    let dir = app.path().app_data_dir().map_err(|e| ApiError::new("io", e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::new("io", format!("{}: {e}", dir.display())))?;
    Ok(dir.join("last-session.bladepilot"))
}

#[tauri::command]
async fn autosave(app: AppHandle, service: Shared<'_>, project: ProjectRequest) -> Result<(), ApiError> {
    let service = Arc::clone(&service);
    let path = autosave_path(&app)?;
    off_main(move || {
        let bytes = service.project_bytes(&project)?;
        std::fs::write(&path, bytes).map_err(|e| ApiError::new("io", format!("{}: {e}", path.display())))
    })
    .await
}

#[tauri::command]
async fn restore_autosave(app: AppHandle, service: Shared<'_>) -> Result<Option<OpenedProject>, ApiError> {
    let service = Arc::clone(&service);
    let path = autosave_path(&app)?;
    off_main(move || match std::fs::read(&path) {
        Ok(bytes) => service.project_from_bytes(&bytes).map(Some),
        Err(_) => Ok(None),
    })
    .await
}

#[tauri::command]
fn discard_autosave(app: AppHandle) -> Result<(), ApiError> {
    let path = autosave_path(&app)?;
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(ApiError::new("io", e.to_string())),
        _ => Ok(()),
    }
}

#[tauri::command]
async fn diagnostics() -> Result<String, ApiError> {
    off_main(|| Ok(service::diagnostics(&udev::status()))).await
}

#[tauri::command]
async fn save_text(path: PathBuf, text: String) -> Result<(), ApiError> {
    off_main(move || std::fs::write(&path, text).map_err(|e| ApiError::new("io", format!("{}: {e}", path.display())))).await
}

#[tauri::command]
fn alignment_design(service: Shared<'_>, width_mm: f64, height_mm: f64) -> DesignInfo {
    service.alignment_design(width_mm, height_mm)
}

#[tauri::command]
async fn weld_items(service: Shared<'_>, items: Vec<PlacedItem>) -> Result<DesignInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.weld_items(&items)).await
}

#[tauri::command]
async fn tidy_design(service: Shared<'_>, id: u32) -> Result<TidyResult, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.tidy_design(id)).await
}

#[tauri::command]
async fn calibration_sheet(service: Shared<'_>, job: JobRequest, path: PathBuf) -> Result<(), ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.calibration_sheet(&job, &path)).await
}

#[tauri::command]
async fn calibration_design(service: Shared<'_>, job: JobRequest) -> Result<DesignInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.calibration_design(&job)).await
}

#[tauri::command]
fn test_design(service: Shared<'_>) -> DesignInfo {
    service.test_design()
}

#[tauri::command]
fn forget_design(service: Shared<'_>, id: u32) {
    service.forget_design(id);
}

#[tauri::command]
async fn plan_job(service: Shared<'_>, job: JobRequest) -> Result<PlanInfo, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.plan(&job)).await
}

#[tauri::command]
async fn simulate_job(service: Shared<'_>, job: JobRequest) -> Result<Simulation, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.simulate(&job)).await
}

#[tauri::command]
async fn scan_devices() -> Result<Vec<DeviceInfo>, ApiError> {
    off_main(service::scan_devices).await
}

#[tauri::command]
async fn device_status(bus: String, address: u8) -> Result<DeviceStatus, ApiError> {
    off_main(move || service::device_status(&bus, address)).await
}

#[tauri::command]
async fn scan_bluetooth() -> Result<Vec<DeviceInfo>, ApiError> {
    off_main(service::scan_bluetooth).await
}

#[tauri::command]
async fn bluetooth_status(service: Shared<'_>, address: String) -> Result<DeviceStatus, ApiError> {
    let service = Arc::clone(&service);
    off_main(move || service.bluetooth_status(&address)).await
}

#[tauri::command]
async fn virtual_status(machine: String) -> Result<DeviceStatus, ApiError> {
    off_main(move || service::virtual_status(&machine)).await
}

#[tauri::command]
async fn start_job(service: Shared<'_>, job: JobRequest, target: Target, events: Channel<JobEvent>) -> Result<(), ApiError> {
    service.start_job(job, target, move |event| {
        // A closed window has nobody to tell.
        let _ = events.send(event);
    })
}

#[tauri::command]
fn control_job(service: Shared<'_>, action: JobAction) {
    service.control_job(action);
}

/// Starts the app.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Service::new()))
        .invoke_handler(tauri::generate_handler![
            app_info,
            usb_access,
            install_usb_access,
            machines,
            startup_files,
            open_design,
            import_design,
            test_design,
            alignment_design,
            weld_items,
            tidy_design,
            calibration_sheet,
            calibration_design,
            save_project,
            open_project,
            autosave,
            restore_autosave,
            discard_autosave,
            diagnostics,
            save_text,
            fonts,
            make_design,
            trace_design,
            contour_path,
            print_sheet,
            forget_design,
            plan_job,
            simulate_job,
            scan_devices,
            device_status,
            scan_bluetooth,
            bluetooth_status,
            virtual_status,
            start_job,
            control_job,
        ])
        .run(tauri::generate_context!())
        .expect("the window could not be opened");
}
