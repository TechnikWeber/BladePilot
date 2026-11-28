// The real backend: the Rust commands of apps/desktop/src-tauri.
import { Channel, invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import demoSvg from './demo.svg?raw';
import type { Backend, DesignInfo, JobEvent, Pinch } from './types';

const DESIGN_EXTENSIONS = ['svg', 'svgz', 'pdf', 'dxf', 'studio', 'studio3', 'png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'];
/** What a drop or "open" accepts: designs and projects. */
const OPEN_EXTENSIONS = ['bladepilot', ...DESIGN_EXTENSIONS];

async function importBytes(name: string, bytes: Uint8Array): Promise<DesignInfo> {
  return invoke('import_design', { name, bytes: Array.from(bytes) });
}

export const tauriBackend: Backend = {
  mock: false,
  appInfo: () => invoke('app_info'),
  machines: () => invoke('machines'),
  usbAccess: () => invoke('usb_access'),
  installUsbAccess: () => invoke('install_usb_access'),
  startupFiles: () => invoke('startup_files'),
  async pickDesigns() {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Designs (SVG, PDF, DXF, Silhouette Studio, pictures)', extensions: DESIGN_EXTENSIONS }],
    });
    const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    const designs: DesignInfo[] = [];
    for (const path of paths) designs.push(await invoke('open_design', { path }));
    return designs;
  },
  openPath: (path) => invoke('open_design', { path }),
  openFile: async (file) => importBytes(file.name, new Uint8Array(await file.arrayBuffer())),
  openDemo: () => importBytes('demo.svg', new TextEncoder().encode(demoSvg)),
  testDesign: () => invoke('test_design'),
  traceDesign: (id, trace) => invoke('trace_design', { id, trace }),
  contourPath: (id, offsetMm) => invoke('contour_path', { id, offsetMm }),
  async printSheet(job) {
    const path = await save({ defaultPath: 'bladepilot-print.pdf', filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (!path) return null;
    await invoke('print_sheet', { job, path });
    return path;
  },
  fonts: () => invoke('fonts'),
  makeDesign: (spec) => invoke('make_design', { spec }),
  forgetDesign: (id) => invoke('forget_design', { id }),
  alignmentDesign: (widthMm, heightMm) => invoke('alignment_design', { widthMm, heightMm }),
  weldItems: (items) => invoke('weld_items', { items }),
  tidyDesign: (id) => invoke('tidy_design', { id }),
  async calibrationSheet(job) {
    const path = await save({ defaultPath: 'bladepilot-calibration.pdf', filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (!path) return null;
    await invoke('calibration_sheet', { job, path });
    return path;
  },
  calibrationDesign: (job) => invoke('calibration_design', { job }),
  async saveProject(project, path) {
    const target = path ?? (await save({ defaultPath: 'project.bladepilot', filters: [{ name: 'BladePilot', extensions: ['bladepilot'] }] }));
    if (!target) return null;
    const file = target.toLowerCase().endsWith('.bladepilot') ? target : `${target}.bladepilot`;
    await invoke('save_project', { path: file, project });
    return file;
  },
  async pickProject() {
    const picked = await open({ multiple: false, directory: false, filters: [{ name: 'BladePilot', extensions: ['bladepilot'] }] });
    if (typeof picked !== 'string') return null;
    return { path: picked, project: await invoke('open_project', { path: picked }) };
  },
  openProject: (path) => invoke('open_project', { path }),
  autosave: (project) => invoke('autosave', { project }),
  restoreAutosave: () => invoke('restore_autosave'),
  discardAutosave: () => invoke('discard_autosave'),
  diagnostics: () => invoke('diagnostics'),
  async saveText(text, name) {
    const path = await save({ defaultPath: name, filters: [{ name: 'Text', extensions: ['txt'] }] });
    if (!path) return null;
    await invoke('save_text', { path, text });
    return path;
  },
  planJob: (job) => invoke('plan_job', { job }),
  simulateJob: (job) => invoke('simulate_job', { job }),
  scanDevices: () => invoke('scan_devices'),
  deviceStatus: (bus, address) => invoke('device_status', { bus, address }),
  scanBluetooth: () => invoke('scan_bluetooth'),
  bluetoothStatus: (address) => invoke('bluetooth_status', { address }),
  virtualStatus: (machine) => invoke('virtual_status', { machine }),
  async startJob(job, target, onEvent) {
    const events = new Channel<JobEvent>();
    events.onmessage = onEvent;
    await invoke('start_job', { job, target, events });
  },
  controlJob: (action) => invoke('control_job', { action }),
  async onFileDrop(onPaths, onHover) {
    return getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === 'enter' || p.type === 'over') onHover(true);
      else if (p.type === 'leave') onHover(false);
      else if (p.type === 'drop') {
        onHover(false);
        const designs = p.paths.filter((path) => OPEN_EXTENSIONS.some((ext) => path.toLowerCase().endsWith('.' + ext)));
        if (designs.length) onPaths(designs);
      }
    });
  },
  onPinch: (onPinch) => listen<Pinch>('pinch', (event) => onPinch(event.payload)),
};

export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
