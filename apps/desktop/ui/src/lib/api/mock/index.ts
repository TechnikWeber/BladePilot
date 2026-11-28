// The mock backend: what the browser demo and the README screenshots run on. The machine table
// and the demo design are the Rust side's own output (see the desktop crate's tests).
import demoJson from './demo.json';
import demoContours from './demo-contours.json';
import testJson from './test-cut.json';
import machinesJson from './machines.json';
import { offsetHull, planMock, simulateMock } from './planner';
import { importSvg } from './svg';
import { madeDesign } from './shapes';
import type { Backend, DesignInfo, JobAction, JobEvent, MachineInfo } from '../types';

const machines = machinesJson as MachineInfo[];
const designs = new Map<number, DesignInfo>();
let nextId = 1;

function keep(design: DesignInfo): DesignInfo {
  const info = { ...design, id: nextId++ };
  designs.set(info.id, info);
  return info;
}

function pickFiles(): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = true;
    input.accept = '.svg,.svgz,.studio,.studio3,.png,.jpg,.jpeg,.webp,.gif,.bmp,image/*';
    input.addEventListener('change', () => resolve(Array.from(input.files ?? [])));
    input.addEventListener('cancel', () => resolve([]));
    input.click();
  });
}

async function readText(file: File): Promise<string> {
  if (file.name.toLowerCase().endsWith('.svgz')) {
    const stream = file.stream().pipeThrough(new DecompressionStream('gzip'));
    return new Response(stream).text();
  }
  return file.text();
}

/** A picture in the browser demo: shown and printed as it is, its outline the picture's box. */
async function pictureDesign(file: File): Promise<DesignInfo> {
  const href = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
  const size = await new Promise<[number, number]>((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve([img.naturalWidth, img.naturalHeight]);
    img.onerror = () => reject({ code: 'image', message: 'the picture could not be read' });
    img.src = href;
  });
  const [w, h] = size.map((px) => (px * 25.4) / 300);
  const d = `M0,0L${w},0L${w},${h}L0,${h}Z`;
  return {
    id: 0,
    name: file.name,
    format: 'image',
    pageMm: [w, h],
    bbox: [0, 0, w, h],
    shapes: [{ d, stroke: '#000000', fill: null, color: '#000000' }],
    colors: [{ color: '#000000', shapes: 1, lengthMm: 2 * (w + h) }],
    hull: [
      [0, 0],
      [w, 0],
      [w, h],
      [0, h],
    ],
    warnings: ['the browser demo outlines a picture by its box; the desktop app traces it'],
    image: { href, widthMm: w, heightMm: h, trace: { source: 'alpha', threshold: 0.5, despeckleMm2: 1, invert: false } },
    printable: true,
  };
}

/** The running mock job: pause, resume and stop act on it. */
let control: { action: JobAction | null; paused: boolean } | null = null;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const mockBackend: Backend = {
  mock: true,
  async appInfo() {
    return { version: __APP_VERSION__, language: navigator.language || 'en', platform: 'web', mock: true };
  },
  async machines() {
    return machines;
  },
  async usbAccess() {
    // `?udev` shows the set-up dialog in the browser demo, for screenshots.
    const missing = new URLSearchParams(location.search).has('udev');
    const command =
      "printf '%s\\n' 'SUBSYSTEM==\"usb\", ATTR{idVendor}==\"0b4d\", TAG+=\"uaccess\"' | sudo tee /etc/udev/rules.d/70-bladepilot.rules > /dev/null && sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=usb";
    return { needed: missing, installed: !missing, command, canElevate: true };
  },
  async installUsbAccess() {
    throw { code: 'elevate', message: 'the browser demo cannot change the system' };
  },
  async startupFiles() {
    return [];
  },
  async pickDesigns() {
    const out: DesignInfo[] = [];
    for (const file of await pickFiles()) out.push(await this.openFile(file));
    return out;
  },
  async openPath(path) {
    throw { code: 'io', message: `${path}: the browser demo cannot read files from disk` };
  },
  async openFile(file) {
    const name = file.name;
    if (/\.studio3?$/i.test(name)) throw { code: 'mockStudio', message: 'Silhouette Studio files need the desktop app' };
    if (/\.(pdf|dxf)$/i.test(name)) throw { code: 'desktopOnly', message: 'PDF and DXF files need the desktop app' };
    if (/\.(png|jpe?g|webp|gif|bmp)$/i.test(name)) return keep(await pictureDesign(file));
    return keep(importSvg(await readText(file), name));
  },
  async openDemo() {
    return keep(demoJson as unknown as DesignInfo);
  },
  async testDesign() {
    const known = [...designs.values()].find((d) => d.name === 'test-cut');
    return known ?? keep(testJson as unknown as DesignInfo);
  },
  async traceDesign(id, trace) {
    const design = designs.get(id);
    if (!design?.image) throw { code: 'notAPicture', message: 'only pictures are traced' };
    const info = { ...design, image: { ...design.image, trace } };
    designs.set(id, info);
    return info;
  },
  async contourPath(id, offsetMm) {
    const design = designs.get(id);
    if (!design) throw { code: 'unknownDesign', message: `no design ${id}` };
    // The demo's real contours, worked out by the Rust side at whole millimetres.
    const known = (demoContours as Record<string, string>)[String(Math.round(offsetMm))];
    if (design.name === 'demo.svg' && known && Math.abs(offsetMm - Math.round(offsetMm)) < 0.05) return known;
    const line = offsetHull(design.hull, offsetMm);
    let d = '';
    for (let k = 0; k + 1 < line.length; k += 2) d += `${k ? 'L' : 'M'}${line[k].toFixed(3)},${line[k + 1].toFixed(3)}`;
    return d + 'Z';
  },
  async printSheet() {
    throw { code: 'desktopOnly', message: 'the browser demo cannot write files' };
  },
  async fonts() {
    return [];
  },
  async makeDesign(spec) {
    return keep(madeDesign(spec));
  },
  async forgetDesign(id) {
    designs.delete(id);
  },
  async planJob(job) {
    return planMock(job, designs, machines).plan;
  },
  async simulateJob(job) {
    return simulateMock(job, designs, machines);
  },
  async scanDevices() {
    // `?connected` pretends a Portrait 4 on USB, for screenshots of a connected machine.
    if (!new URLSearchParams(location.search).has('connected')) return [];
    return [{ machine: 'portrait4', name: 'Silhouette Portrait 4', vendorId: 0x0b4d, productId: 0x113f, serial: null, bus: '1', address: 11, bluetooth: null }];
  },
  async deviceStatus() {
    if (new URLSearchParams(location.search).has('connected')) return { version: 'Portrait 4 V1.05', status: 'ready', tools: ['autoBlade'] };
    throw { code: 'usb', message: 'no USB in the browser demo' };
  },
  async scanBluetooth() {
    return [];
  },
  async bluetoothStatus() {
    throw { code: 'bluetooth', message: 'no Bluetooth in the browser demo' };
  },
  async virtualStatus(machine) {
    const m = machines.find((x) => x.id === machine);
    const name = (m?.name ?? machine).replace(/^Silhouette /, '').toUpperCase();
    return { version: `${name} V0.00 (virtual)`, status: 'ready', tools: ['autoBlade'] };
  },
  async startJob(job, _target, onEvent) {
    if (control) throw { code: 'busy', message: 'another job is running' };
    const sim = simulateMock(job, designs, machines);
    control = { action: null, paused: false };
    const ctl = control;
    const emit = (e: JobEvent) => onEvent(e);
    void (async () => {
      const total = sim.plan.bytes;
      const machine = machines.find((m) => m.id === job.machine);
      const name = (machine?.name ?? job.machine).replace(/^Silhouette /, '').toUpperCase();
      // Time per stroke at the virtual plotter's pace, mm/s.
      const durations = sim.strokes.map((s) => {
        let len = 0;
        for (let k = 2; k < s.points.length; k += 2) len += Math.hypot(s.points[k] - s.points[k - 2], s.points[k + 1] - s.points[k - 1]);
        return (len / (s.cut ? 120 : 360)) * 1000;
      });
      const all = durations.reduce((a, b) => a + b, 0) || 1;
      await sleep(300);
      emit({ kind: 'connected', version: `${name} V0.00 (virtual)` });
      await sleep(200);
      emit({ kind: 'ready' });
      let pass = -1;
      let elapsed = 0;
      for (let i = 0; i < sim.strokes.length; i++) {
        const s = sim.strokes[i];
        if (s.pass !== null && s.pass !== pass) {
          const info = sim.plan.passes[s.pass];
          if (s.pass > 0 && info.pauseBefore) {
            ctl.paused = true;
            emit({ kind: 'paused', beforePass: s.pass });
            while (ctl.paused && ctl.action !== 'stop') await sleep(60);
            if (ctl.action === 'stop') break;
            emit({ kind: 'resumed' });
          }
          pass = s.pass;
          emit({ kind: 'pass', index: pass });
        }
        if (ctl.action === 'stop') break;
        if (ctl.action === 'pause' && !s.cut) {
          ctl.paused = true;
          ctl.action = null;
          emit({ kind: 'paused', beforePass: null });
          while (ctl.paused && ctl.action !== 'stop') await sleep(60);
          if (ctl.action === 'stop') break;
          emit({ kind: 'resumed' });
        }
        await sleep(Math.min(durations[i], 1500));
        elapsed += durations[i];
        emit({ kind: 'sending', sent: Math.round((total * elapsed) / all), total });
      }
      const stopped = ctl.action === 'stop';
      if (!stopped) {
        emit({ kind: 'finishing' });
        await sleep(400);
      } else await sleep(300);
      control = null;
      emit(stopped ? { kind: 'stopped' } : { kind: 'done' });
    })();
  },
  async controlJob(action) {
    if (!control) return;
    if (action === 'resume') {
      control.paused = false;
      control.action = null;
    } else control.action = action;
  },
  async onFileDrop() {
    return () => {};
  },
};
