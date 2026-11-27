// The real backend: the Rust commands of apps/desktop/src-tauri.
import { Channel, invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import demoSvg from './demo.svg?raw';
import type { Backend, DesignInfo, JobEvent } from './types';

const DESIGN_EXTENSIONS = ['svg', 'svgz', 'studio', 'studio3'];

async function importBytes(name: string, bytes: Uint8Array): Promise<DesignInfo> {
  return invoke('import_design', { name, bytes: Array.from(bytes) });
}

export const tauriBackend: Backend = {
  mock: false,
  appInfo: () => invoke('app_info'),
  machines: () => invoke('machines'),
  startupFiles: () => invoke('startup_files'),
  async pickDesigns() {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Designs (SVG, Silhouette Studio)', extensions: DESIGN_EXTENSIONS }],
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
  forgetDesign: (id) => invoke('forget_design', { id }),
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
        const designs = p.paths.filter((path) => DESIGN_EXTENSIONS.some((ext) => path.toLowerCase().endsWith('.' + ext)));
        if (designs.length) onPaths(designs);
      }
    });
  },
};

export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
