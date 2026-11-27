// The mock backend: what the browser demo and the README screenshots run on. The machine table
// and the demo design are the Rust side's own output (see the desktop crate's tests).
import demoJson from './demo.json';
import testJson from './test-cut.json';
import machinesJson from './machines.json';
import { planMock, simulateMock } from './planner';
import { importSvg } from './svg';
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
    input.accept = '.svg,.svgz,.studio,.studio3,image/svg+xml';
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
    return keep(importSvg(await readText(file), name));
  },
  async openDemo() {
    return keep(demoJson as unknown as DesignInfo);
  },
  async testDesign() {
    const known = [...designs.values()].find((d) => d.name === 'test-cut');
    return known ?? keep(testJson as unknown as DesignInfo);
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
    return [];
  },
  async deviceStatus() {
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
