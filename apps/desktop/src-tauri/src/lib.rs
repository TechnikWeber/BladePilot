//! BladePilot's desktop app. The commands below only hand their arguments to [`service`], which
//! does the work with the workspace crates; the web front end in `ui/` calls them.
//!
//! Everything slow runs off the main thread: imports, plans and simulations on Tauri's blocking
//! pool, a job on a thread of its own that reports through a channel. The window never waits for
//! the machine.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::State;
use tauri::ipc::Channel;

pub mod model;
pub mod service;
pub mod udev;

use model::{
    ApiError, AppInfo, DesignInfo, DeviceInfo, DeviceStatus, JobAction, JobEvent, JobRequest, MachineInfo, PlanInfo, Simulation, Target, UsbAccess,
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
