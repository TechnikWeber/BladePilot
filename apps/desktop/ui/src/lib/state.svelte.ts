// The app's state: machine and mat, the designs on the mat, the colour list, the plan, the
// preview and the job. Components read and change it directly; Svelte's runes keep the views in
// step.
import { backend } from './api';
import type {
  ApiError,
  AppInfo,
  ColorInfo,
  DesignInfo,
  DeviceInfo,
  DeviceStatus,
  JobEvent,
  JobRequest,
  MachineInfo,
  MatInfo,
  PassInfo,
  PlanInfo,
  Settings,
  Simulation,
  Target,
  Tool,
} from './api/types';
import { type Box, type Placement, designSize, inside, normalizeAngle, placedBox, placementMatrix, union } from './geometry';
import { errorText, t } from './i18n/index.svelte';

export interface Item extends Placement {
  id: number;
  design: number;
}

export interface ColorPrefs {
  settings: Settings;
  skip: boolean;
  pauseBefore: boolean;
}

export interface ColorRow {
  color: string | null;
  key: string;
  prefs: ColorPrefs;
  info: ColorInfo;
  /** The planned pass of this colour, when it is cut. */
  pass: PassInfo | null;
  passIndex: number;
}

export type SendPhase = 'setup' | 'connecting' | 'running' | 'paused' | 'finishing' | 'stopping' | 'done' | 'stopped' | 'failed';

export interface SendState {
  phase: SendPhase;
  /** The designs on the mat, or the test cut. */
  mode: 'job' | 'test';
  target: Target;
  sent: number;
  total: number;
  pass: number;
  /** The pass a pause waits before, `null` for a pause on request. */
  pausedBefore: number | null;
  pausing: boolean;
  version: string | null;
  error: ApiError | null;
  /** The plan the job was started with. */
  plan: PlanInfo | null;
}

export interface PreviewState {
  loading: boolean;
  sim: Simulation | null;
  error: ApiError | null;
  /** Playback position in milliseconds of the timeline at 1×. */
  time: number;
  playing: boolean;
  speed: number;
}

/** The preview's strokes on a time line: when each starts and how long it takes at 1×. */
export interface Timeline {
  strokes: { cut: boolean; pass: number | null; points: number[]; cum: number[]; t0: number; dur: number }[];
  total: number;
  /** Start time of every pass. */
  passStarts: number[];
}

/** Playback pace at 1×: cuts and lifted moves in mm/s, and a short moment for every tool lift. */
const PREVIEW_CUT_MM_S = 45;
const PREVIEW_TRAVEL_MM_S = 180;
const PREVIEW_LIFT_MS = 60;

export function buildTimeline(sim: Simulation | null | undefined): Timeline | null {
  if (!sim) return null;
  let clock = 0;
  const passStarts: number[] = [];
  const strokes = sim.strokes.map((s) => {
    const cum = [0];
    for (let k = 2; k < s.points.length; k += 2) cum.push(cum[cum.length - 1] + Math.hypot(s.points[k] - s.points[k - 2], s.points[k + 1] - s.points[k - 1]));
    const len = cum[cum.length - 1];
    const dur = (len / (s.cut ? PREVIEW_CUT_MM_S : PREVIEW_TRAVEL_MM_S)) * 1000 + (s.cut ? PREVIEW_LIFT_MS : 0);
    if (s.pass !== null && passStarts[s.pass] === undefined) passStarts[s.pass] = clock;
    const entry = { cut: s.cut, pass: s.pass, points: s.points, cum, t0: clock, dur };
    clock += dur;
    return entry;
  });
  return { strokes, total: clock, passStarts };
}

/** Where the next test cut goes and with which settings. */
export interface TestCut {
  x: number;
  y: number;
  settings: Settings;
  /** The colour whose settings are tried, for the dialog. */
  color: string | null;
}

/** Edge of the test pattern, mm (as `bladepilot_job::TEST_SIZE_MM`). */
export const TEST_SIZE_MM = 12;

export interface Toast {
  id: number;
  text: string;
  kind: 'error' | 'info';
}

export interface View {
  /** Screen pixels per millimetre. */
  scale: number;
  /** Screen position of the mat origin. */
  x: number;
  y: number;
}

const PREFS_KEY = 'bladepilot.prefs.v1';

export function defaultSettings(tool: Tool = 'blade'): Settings {
  return { tool, force: null, speed: null, depth: null, repeats: 1, overcutMm: 0, bladeOffsetMm: 0.9 };
}

export function colorKey(color: string | null): string {
  return color ?? 'none';
}

interface StoredPrefs {
  machineId?: string;
  matId?: string;
  unit?: 'mm' | 'in';
  byColor?: boolean;
  single?: Settings;
  colors?: Record<string, ColorPrefs>;
}

function loadPrefs(): StoredPrefs {
  try {
    return JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}') as StoredPrefs;
  } catch {
    return {};
  }
}

let nextItemId = 1;
let nextToastId = 1;
let planSeq = 0;

class AppState {
  info = $state<AppInfo | null>(null);
  machines = $state<MachineInfo[]>([]);
  machineId = $state('portrait3');
  matId = $state('8x12');
  unit = $state<'mm' | 'in'>('mm');

  designs = $state<Record<number, DesignInfo>>({});
  items = $state<Item[]>([]);
  selected = $state<number | null>(null);
  /** A drag on the mat is under way: planning waits for its end. */
  dragging = $state(false);

  byColor = $state(true);
  manualOrder = $state(false);
  order = $state<(string | null)[]>([]);
  prefs = $state<Record<string, ColorPrefs>>({});
  single = $state<Settings>(defaultSettings());

  plan = $state<PlanInfo | null>(null);
  planError = $state<ApiError | null>(null);
  planning = $state(false);

  devices = $state<DeviceInfo[]>([]);
  target = $state('virtual');
  status = $state<DeviceStatus | null>(null);
  statusError = $state<ApiError | null>(null);
  checking = $state(false);
  scanning = $state(false);
  understood = $state(false);

  view = $state<View>({ scale: 2, x: 40, y: 40 });
  viewport = $state({ w: 800, h: 600 });
  fitRequest = $state(0);

  preview = $state<PreviewState | null>(null);
  send = $state<SendState | null>(null);
  test = $state<TestCut | null>(null);
  testDesign = $state<DesignInfo | null>(null);
  /** Test cuts already made this session: the next one goes elsewhere. */
  private testSpots: { x: number; y: number }[] = [];
  toasts = $state<Toast[]>([]);
  dropping = $state(false);

  private past: Item[][] = [];
  private future: Item[][] = [];
  canUndo = $state(false);
  canRedo = $state(false);

  timeline = $derived(buildTimeline(this.preview?.sim));

  machine = $derived(this.machines.find((m) => m.id === this.machineId) ?? this.machines[0]);
  mat = $derived<MatInfo | undefined>(this.machine?.mats.find((m) => m.id === this.matId) ?? this.machine?.mats[0]);
  /** Media without a mat is shown as long as the designs need, at least a mat's length. */
  rollLength = $derived.by(() => {
    const box = this.itemsBox;
    return Math.max(304.8, Math.ceil(((box?.y1 ?? 0) + 40) / 25.4) * 25.4);
  });
  /** Cutting area in mm. */
  area = $derived.by(() => {
    const mat = this.mat;
    if (!mat) return { w: 203, h: 304.8 };
    return { w: mat.areaWidthMm, h: mat.id === 'none' ? this.rollLength : mat.areaHeightMm };
  });
  /** The sheet drawn under the cutting area. */
  sheet = $derived.by(() => {
    const mat = this.mat;
    if (!mat) return { w: 203.2, h: 304.8 };
    return { w: mat.widthMm, h: mat.id === 'none' ? this.rollLength : mat.heightMm };
  });
  selectedItem = $derived(this.items.find((i) => i.id === this.selected) ?? null);

  boxes = $derived.by(() => {
    const out: Record<number, Box> = {};
    for (const item of this.items) {
      const design = this.designs[item.design];
      if (design) out[item.id] = placedBox(item, design);
    }
    return out;
  });
  itemsBox = $derived.by(() => {
    let box: Box | null = null;
    for (const item of this.items) box = union(box, this.boxesOf(item));
    return box;
  });
  /** Items that reach outside the cutting area. */
  outside = $derived.by(() => {
    const out = new Set<number>();
    const area = this.mat ? { w: this.mat.areaWidthMm, h: this.mat.id === 'none' ? Infinity : this.mat.areaHeightMm } : this.area;
    for (const item of this.items) {
      const box = this.boxes[item.id];
      if (box && !inside(box, area)) out.add(item.id);
    }
    return out;
  });

  /** The cut colours on the mat, in first-seen order, with shape counts and lengths summed. */
  colorInfos = $derived.by(() => {
    const out: ColorInfo[] = [];
    for (const item of this.items) {
      const design = this.designs[item.design];
      if (!design) continue;
      const k = Math.sqrt(Math.abs(item.sx * item.sy));
      for (const c of design.colors) {
        const entry = out.find((e) => e.color === c.color);
        if (entry) {
          entry.shapes += c.shapes;
          entry.lengthMm += c.lengthMm * k;
        } else out.push({ color: c.color, shapes: c.shapes, lengthMm: c.lengthMm * k });
      }
    }
    return out;
  });

  /** The colours in the order the job asks for: the user's order when it is manual. */
  requestOrder = $derived.by(() => {
    const natural = this.colorInfos.map((c) => c.color);
    if (!this.manualOrder) return natural;
    const kept = this.order.filter((c) => natural.includes(c));
    return [...kept, ...natural.filter((c) => !kept.includes(c))];
  });

  /** The colour list as shown: in cutting order, left-out colours at the end. */
  rows = $derived.by((): ColorRow[] => {
    const infos = this.colorInfos;
    const plan = this.plan;
    let order = this.requestOrder;
    if (!this.manualOrder && plan && this.byColor) {
      const planned = plan.passes.map((p) => p.color).filter((c) => order.includes(c));
      order = [...planned, ...order.filter((c) => !planned.includes(c))];
    }
    return order.map((color) => {
      const info = infos.find((i) => i.color === color) ?? { color, shapes: 0, lengthMm: 0 };
      const passIndex = plan && this.byColor ? plan.passes.findIndex((p) => p.color === color) : -1;
      return { color, key: colorKey(color), prefs: this.prefsOf(color), info, pass: passIndex >= 0 && plan ? plan.passes[passIndex] : null, passIndex };
    });
  });

  job = $derived.by((): JobRequest | null => {
    const machine = this.machine;
    const mat = this.mat;
    if (!machine || !mat || !this.items.length) return null;
    const items = [];
    for (const item of this.items) {
      const design = this.designs[item.design];
      if (design) items.push({ design: item.design, matrix: placementMatrix(item, design) });
    }
    return {
      machine: machine.id,
      mat: mat.id,
      items,
      byColor: this.byColor,
      manualOrder: this.byColor && this.manualOrder,
      settings: $state.snapshot(this.single),
      colors: this.requestOrder.map((color) => {
        const p = this.prefsOf(color);
        return { color, settings: $state.snapshot(p.settings), skip: p.skip, pauseBefore: p.pauseBefore };
      }),
    };
  });

  /** The test cut as a job: the pattern alone, one pass with the chosen settings. */
  testJob = $derived.by((): JobRequest | null => {
    const test = this.test;
    const design = this.testDesign;
    if (!test || !design || !this.machine || !this.mat) return null;
    return {
      machine: this.machine.id,
      mat: this.mat.id,
      items: [{ design: design.id, matrix: [1, 0, 0, 1, test.x, test.y] }],
      byColor: false,
      manualOrder: false,
      settings: $state.snapshot(test.settings),
      colors: [],
    };
  });

  boxesOf(item: Item): Box | null {
    return this.boxes[item.id] ?? null;
  }

  prefsOf(color: string | null): ColorPrefs {
    return this.prefs[colorKey(color)] ?? { settings: defaultSettings(), skip: false, pauseBefore: false };
  }

  /** The stored prefs of a colour, created on first change. */
  editPrefs(color: string | null): ColorPrefs {
    const key = colorKey(color);
    if (!this.prefs[key]) this.prefs[key] = { settings: defaultSettings(), skip: false, pauseBefore: false };
    return this.prefs[key];
  }

  async init(): Promise<void> {
    const stored = loadPrefs();
    if (stored.unit) this.unit = stored.unit;
    if (stored.byColor !== undefined) this.byColor = stored.byColor;
    if (stored.single) this.single = { ...defaultSettings(), ...stored.single };
    if (stored.colors) this.prefs = stored.colors;
    const [info, machines] = await Promise.all([backend.appInfo(), backend.machines()]);
    this.info = info;
    this.machines = machines;
    const machine = machines.find((m) => m.id === stored.machineId) ?? machines[0];
    this.machineId = machine.id;
    this.matId = machine.mats.some((m) => m.id === stored.matId) ? (stored.matId as string) : machine.defaultMat;
    if (!stored.unit && /^en-(US|LR|MM)/i.test(info.language)) this.unit = 'in';
  }

  savePrefs(): void {
    const data: StoredPrefs = {
      machineId: this.machineId,
      matId: this.matId,
      unit: this.unit,
      byColor: this.byColor,
      single: $state.snapshot(this.single),
      colors: $state.snapshot(this.prefs),
    };
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(data));
    } catch {
      // Private browsing: settings are not kept, nothing else changes.
    }
  }

  setMachine(id: string): void {
    const machine = this.machines.find((m) => m.id === id);
    if (!machine) return;
    this.machineId = id;
    if (!machine.mats.some((m) => m.id === this.matId)) this.matId = machine.defaultMat;
    this.status = null;
    this.statusError = null;
    this.fitRequest++;
  }

  setMat(id: string): void {
    this.matId = id;
    this.fitRequest++;
  }

  /** Zooms around a point of the view (its centre by default). */
  zoomBy(factor: number, sx = this.viewport.w / 2, sy = this.viewport.h / 2): void {
    const v = this.view;
    const scale = Math.max(0.4, Math.min(60, v.scale * factor));
    const k = scale / v.scale;
    this.view = { scale, x: sx - (sx - v.x) * k, y: sy - (sy - v.y) * k };
  }

  toast(text: string, kind: Toast['kind'] = 'error'): void {
    const id = nextToastId++;
    this.toasts.push({ id, text, kind });
    setTimeout(() => (this.toasts = this.toasts.filter((toast) => toast.id !== id)), kind === 'error' ? 7000 : 4000);
  }

  // History

  checkpoint(): void {
    this.past.push($state.snapshot(this.items));
    if (this.past.length > 100) this.past.shift();
    this.future = [];
    this.canUndo = true;
    this.canRedo = false;
  }

  undo(): void {
    const prev = this.past.pop();
    if (!prev) return;
    this.future.push($state.snapshot(this.items));
    this.items = prev;
    this.afterHistory();
  }

  redo(): void {
    const next = this.future.pop();
    if (!next) return;
    this.past.push($state.snapshot(this.items));
    this.items = next;
    this.afterHistory();
  }

  private afterHistory(): void {
    if (!this.items.some((i) => i.id === this.selected)) this.selected = null;
    this.canUndo = this.past.length > 0;
    this.canRedo = this.future.length > 0;
  }

  // Designs and items

  /** Puts a freshly opened design on the mat: where its page says when that fits, else top left. */
  place(design: DesignInfo): void {
    if (!design.shapes.length) {
      this.toast(`${t('error.open', { name: design.name })}: ${t('error.nothingInFile')}`);
      return;
    }
    this.designs[design.id] = design;
    const { w, h } = designSize(design);
    const [x0, y0] = design.bbox;
    const area = this.area;
    let cx = x0 + w / 2;
    let cy = y0 + h / 2;
    const fits = x0 >= 0 && y0 >= 0 && x0 + w <= area.w && y0 + h <= area.h;
    if (!fits) {
      cx = Math.min(10, Math.max(0, (area.w - w) / 2)) + w / 2;
      cy = 10 + h / 2;
    }
    // Not exactly on top of another item.
    while (this.items.some((i) => Math.abs(i.cx - cx) < 0.5 && Math.abs(i.cy - cy) < 0.5)) {
      cx += 10;
      cy += 10;
    }
    const item: Item = { id: nextItemId++, design: design.id, cx, cy, angle: 0, sx: 1, sy: 1 };
    this.items.push(item);
    this.selected = item.id;
    for (const w of design.warnings) this.toast(`${design.name}: ${w}`, 'info');
  }

  async open(load: () => Promise<DesignInfo[]>, name?: string): Promise<void> {
    try {
      const designs = await load();
      if (!designs.length) return;
      this.checkpoint();
      for (const d of designs) this.place(d);
    } catch (e) {
      const file = name?.split(/[\\/]/).pop();
      this.toast(file ? `${t('error.open', { name: file })}: ${errorText(e)}` : errorText(e));
    }
  }

  async openDemo(): Promise<void> {
    const red = '#e5484d';
    const blue = '#2f6fde';
    const black = '#1f2328';
    if (!this.prefs[red]) this.prefs[red] = { settings: { ...defaultSettings('pen') }, skip: false, pauseBefore: false };
    if (!this.prefs[blue]) this.prefs[blue] = { settings: { ...defaultSettings(), force: 10, speed: 5, depth: 2 }, skip: false, pauseBefore: false };
    if (!this.prefs[black]) this.prefs[black] = { settings: { ...defaultSettings(), force: 20, speed: 5, depth: 4, repeats: 2 }, skip: false, pauseBefore: false };
    this.byColor = true;
    await this.open(async () => [await backend.openDemo()]);
  }

  removeSelected(): void {
    if (this.selected === null) return;
    this.checkpoint();
    this.items = this.items.filter((i) => i.id !== this.selected);
    this.selected = null;
  }

  duplicateSelected(): void {
    const item = this.selectedItem;
    const box = item ? this.boxes[item.id] : null;
    if (!item || !box) return;
    this.checkpoint();
    // Next to the original where there is room: right, below, else a little offset.
    const w = box.x1 - box.x0;
    const h = box.y1 - box.y0;
    const gap = 5;
    const free = (dx: number, dy: number) => {
      const b = { x0: box.x0 + dx, y0: box.y0 + dy, x1: box.x1 + dx, y1: box.y1 + dy };
      const fits = b.x0 >= 0 && b.y0 >= 0 && b.x1 <= this.area.w && b.y1 <= this.area.h;
      const clear = Object.values(this.boxes).every((o) => o.x1 <= b.x0 || o.x0 >= b.x1 || o.y1 <= b.y0 || o.y0 >= b.y1);
      return fits && clear;
    };
    const [dx, dy] = [[w + gap, 0], [0, h + gap], [-(w + gap), 0], [0, -(h + gap)]].find(([x, y]) => free(x, y)) ?? [10, 10];
    const copy: Item = { ...$state.snapshot(item), id: nextItemId++, cx: item.cx + dx, cy: item.cy + dy };
    this.items.push(copy);
    this.selected = copy.id;
  }

  rotateSelected(degrees: number): void {
    const item = this.selectedItem;
    if (!item) return;
    this.checkpoint();
    item.angle = normalizeAngle(item.angle + degrees);
  }

  mirrorSelected(horizontal: boolean): void {
    const item = this.selectedItem;
    if (!item) return;
    this.checkpoint();
    // A mirror across the screen axis: the turn goes the other way.
    if (horizontal) item.sx = -item.sx;
    else item.sy = -item.sy;
    item.angle = normalizeAngle(-item.angle);
  }

  nudgeSelected(dx: number, dy: number): void {
    const item = this.selectedItem;
    if (!item) return;
    this.checkpoint();
    item.cx += dx;
    item.cy += dy;
  }

  /** Moves the selection so its box starts at (x, y). */
  moveSelectedTo(x: number | null, y: number | null): void {
    const item = this.selectedItem;
    const box = item ? this.boxes[item.id] : null;
    if (!item || !box) return;
    this.checkpoint();
    if (x !== null) item.cx += x - box.x0;
    if (y !== null) item.cy += y - box.y0;
  }

  centerSelected(): void {
    const item = this.selectedItem;
    const box = item ? this.boxes[item.id] : null;
    if (!item || !box) return;
    this.checkpoint();
    item.cx += this.area.w / 2 - (box.x0 + box.x1) / 2;
    item.cy += this.area.h / 2 - (box.y0 + box.y1) / 2;
  }

  // Planning

  async replan(job: JobRequest): Promise<void> {
    const seq = ++planSeq;
    this.planning = true;
    try {
      const plan = await backend.planJob(job);
      if (seq !== planSeq) return;
      this.plan = plan;
      this.planError = null;
    } catch (e) {
      if (seq !== planSeq) return;
      this.plan = null;
      this.planError = (e as ApiError) ?? { code: 'generic', message: String(e) };
    } finally {
      if (seq === planSeq) this.planning = false;
    }
  }

  clearPlan(): void {
    planSeq++;
    this.plan = null;
    this.planError = null;
    this.planning = false;
  }

  // Machine

  async scan(): Promise<void> {
    this.scanning = true;
    try {
      this.devices = await backend.scanDevices();
      if (this.target !== 'virtual' && !this.devices.some((d) => deviceKey(d) === this.target)) this.target = 'virtual';
    } catch (e) {
      this.devices = [];
      if (!backend.mock) this.toast(errorText(e));
    } finally {
      this.scanning = false;
    }
  }

  device = $derived(this.devices.find((d) => deviceKey(d) === this.target) ?? null);

  async checkStatus(): Promise<void> {
    this.checking = true;
    this.statusError = null;
    try {
      const device = this.device;
      this.status = device ? await backend.deviceStatus(device.bus, device.address) : await backend.virtualStatus(this.machineId);
    } catch (e) {
      this.status = null;
      this.statusError = e as ApiError;
    } finally {
      this.checking = false;
    }
  }

  targetValue(): Target {
    const device = this.device;
    return device ? { kind: 'usb', bus: device.bus, address: device.address } : { kind: 'virtual' };
  }

  // Preview

  async openPreview(): Promise<void> {
    const job = this.job;
    if (!job) return;
    this.preview = { loading: true, sim: null, error: null, time: 0, playing: false, speed: 1 };
    try {
      const sim = await backend.simulateJob(job);
      if (!this.preview) return;
      this.preview.sim = sim;
      this.preview.loading = false;
      // About ten seconds for the whole job, in steps of 1, 2, 4, …
      const total = buildTimeline(sim)?.total ?? 0;
      this.preview.speed = Math.min(64, Math.max(1, 2 ** Math.round(Math.log2(Math.max(1, total / 10000)))));
      this.preview.playing = true;
    } catch (e) {
      if (!this.preview) return;
      this.preview.loading = false;
      this.preview.error = e as ApiError;
    }
  }

  closePreview(): void {
    this.preview = null;
  }

  // Test cut

  /** A free 12 mm spot inside the cutting area, from the top right, clear of designs and earlier tests. */
  freeTestSpot(): { x: number; y: number } {
    const size = TEST_SIZE_MM;
    const gap = 4;
    const area = this.area;
    const taken: Box[] = [
      ...Object.values(this.boxes),
      ...this.testSpots.map((p) => ({ x0: p.x, y0: p.y, x1: p.x + size, y1: p.y + size })),
    ];
    for (let y = gap; y + size <= area.h - gap; y += size + gap) {
      for (let x = area.w - size - gap; x >= gap; x -= size + gap) {
        const b = { x0: x - 2, y0: y - 2, x1: x + size + 2, y1: y + size + 2 };
        if (taken.every((o) => o.x1 <= b.x0 || o.x0 >= b.x1 || o.y1 <= b.y0 || o.y0 >= b.y1)) return { x, y };
      }
    }
    return { x: gap, y: gap };
  }

  /** Opens the send dialog for a test cut with these settings. */
  async openTestCut(settings: Settings, color: string | null): Promise<void> {
    try {
      if (!this.testDesign) this.testDesign = await backend.testDesign();
    } catch (e) {
      this.toast(errorText(e));
      return;
    }
    const spot = this.test ?? this.freeTestSpot();
    this.test = { x: spot.x, y: spot.y, settings: { ...$state.snapshot(settings) }, color };
    this.preview = null;
    this.send = { phase: 'setup', mode: 'test', target: this.targetValue(), sent: 0, total: 0, pass: 0, pausedBefore: null, pausing: false, version: null, error: null, plan: null };
    await this.planTest();
  }

  async planTest(): Promise<void> {
    const job = this.testJob;
    const send = this.send;
    if (!job || !send || send.mode !== 'test') return;
    try {
      send.plan = await backend.planJob(job);
      send.error = null;
    } catch (e) {
      send.plan = null;
      send.error = e as ApiError;
    }
  }

  moveTest(x: number | null, y: number | null): void {
    if (!this.test) return;
    if (x !== null) this.test.x = Math.max(0, Math.min(this.area.w - TEST_SIZE_MM, x));
    if (y !== null) this.test.y = Math.max(0, Math.min(this.area.h - TEST_SIZE_MM, y));
    void this.planTest();
  }

  nextTestSpot(): void {
    if (!this.test) return;
    this.testSpots.push({ x: this.test.x, y: this.test.y });
    const spot = this.freeTestSpot();
    this.test.x = spot.x;
    this.test.y = spot.y;
    void this.planTest();
  }

  // Sending

  openSend(): void {
    if (!this.job) return;
    this.preview = null;
    this.send = { phase: 'setup', mode: 'job', target: this.targetValue(), sent: 0, total: 0, pass: 0, pausedBefore: null, pausing: false, version: null, error: null, plan: this.plan };
  }

  async startSend(): Promise<void> {
    const send = this.send;
    const job = send?.mode === 'test' ? this.testJob : this.job;
    if (!job || !send) return;
    send.target = this.targetValue();
    if (send.mode === 'job') send.plan = this.plan;
    send.phase = 'connecting';
    send.sent = 0;
    send.total = send.plan?.bytes ?? 0;
    send.error = null;
    send.pass = 0;
    try {
      await backend.startJob(job, send.target, (e) => this.onJobEvent(e));
    } catch (e) {
      send.phase = 'failed';
      send.error = e as ApiError;
    }
  }

  private onJobEvent(e: JobEvent): void {
    const s = this.send;
    if (!s) return;
    switch (e.kind) {
      case 'connected':
        s.version = e.version;
        break;
      case 'ready':
        s.phase = 'running';
        break;
      case 'pass':
        s.pass = e.index;
        break;
      case 'sending':
        s.sent = e.sent;
        s.total = e.total;
        break;
      case 'searchingMarks':
        break;
      case 'paused':
        s.phase = 'paused';
        s.pausing = false;
        s.pausedBefore = e.beforePass;
        if (e.beforePass !== null) s.pass = e.beforePass;
        break;
      case 'resumed':
        s.phase = 'running';
        s.pausedBefore = null;
        break;
      case 'finishing':
        s.phase = 'finishing';
        s.sent = s.total;
        break;
      case 'done':
        s.phase = 'done';
        s.sent = s.total;
        // The next test goes next to this one.
        if (s.mode === 'test' && this.test) this.testSpots.push({ x: this.test.x, y: this.test.y });
        break;
      case 'stopped':
        s.phase = 'stopped';
        break;
      case 'failed':
        s.phase = 'failed';
        s.error = e.error;
        break;
    }
  }

  async control(action: 'pause' | 'resume' | 'stop'): Promise<void> {
    const s = this.send;
    if (!s) return;
    if (action === 'pause') s.pausing = true;
    if (action === 'stop') s.phase = 'stopping';
    await backend.controlJob(action);
  }

  closeSend(): void {
    const s = this.send;
    if (s && ['connecting', 'running', 'paused', 'finishing', 'stopping'].includes(s.phase)) return;
    this.send = null;
  }
}

export function deviceKey(d: DeviceInfo): string {
  return `usb:${d.bus}:${d.address}`;
}

export const app = new AppState();
