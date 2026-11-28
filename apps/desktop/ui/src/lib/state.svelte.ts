// The app's state: machine and mat, the designs on the mat, the colour list, the plan, the
// preview and the job. Components read and change it directly; Svelte's runes keep the views in
// step.
import { backend } from './api';
import type {
  ApiError,
  AppInfo,
  ItemCut,
  MadeSpec,
  PrintSetup,
  WeedRequest,
  TraceRequest,
  UsbAccess,
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
import type { Key } from './i18n/en';
import { BUILTIN_MATERIALS, type Material, loadMaterials, storeMaterials } from './materials';

export interface Item extends Placement {
  id: number;
  design: number;
  /** What of the design is cut. */
  cut: ItemCut;
  /** Print and cut: printed on the sheet. */
  print: boolean;
  /** Weeding box (and lines) around the design. */
  weed: WeedRequest | null;
}

/** The colour contour cuts get, a pass of their own (as `CONTOUR_COLOR` on the Rust side). */
export const CONTOUR_COLOR = '#ff00ff';
/** The colour of weeding boxes and lines (as `WEED_COLOR` on the Rust side). */
export const WEED_COLOR = '#00a0a0';
/** Room left between designs when they are arranged, mm. */
export const ARRANGE_GAP_MM = 3;

/** The make dialog: a new shape or text, or a change to the item `itemId`. */
export interface MakeState {
  spec: MadeSpec;
  itemId: number | null;
}

export const PAPERS = { a4: { w: 210, h: 297 }, letter: { w: 215.9, h: 279.4 } } as const;
/** Marks: a square at the top left, brackets at the top right and bottom left (as bladepilot-print). */
export const MARK = { square: 5, bracket: 20, line: 0.5, clearance: 3 } as const;

/** Where the marks sit: their outer extent, mm (as `Marks::layout`). */
export interface MarkLayout {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export function markLayout(paper: { w: number; h: number }, area: { w: number; h: number }, inset: number): MarkLayout | null {
  const right = Math.min(paper.w, area.w) - inset;
  const bottom = Math.min(paper.h, area.h) - inset;
  const least = 2 * (MARK.bracket + MARK.clearance) + 10;
  if (inset < 0 || right - inset < least || bottom - inset < least) return null;
  return { left: inset, top: inset, right, bottom };
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
/** Distance of the first test from the edges of the cutting area, mm: well inside the mat's grid. */
export const TEST_MARGIN_MM = 15;

/** The corner the test cuts start in: top or bottom, left or right. */
export type TestCorner = 'tl' | 'tr' | 'bl' | 'br';

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
const USB_QUIET_KEY = 'bladepilot.usbAccess.quiet';

export function defaultSettings(tool: Tool = 'blade'): Settings {
  return { tool, force: null, speed: null, depth: null, repeats: 1, overcutMm: 0, bladeOffsetMm: 0.9 };
}

export function colorKey(color: string | null): string {
  return color ?? 'none';
}

interface StoredPrefs {
  print?: PrintSetup;
  testCorner?: TestCorner;
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
  usb = $state<UsbAccess | null>(null);
  /** The USB set-up dialog is open. */
  usbDialog = $state(false);
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
  /** Print and cut is on: designs are printed with registration marks, then cut. */
  printMode = $state(false);
  printSetup = $state<PrintSetup>({ paper: 'a4', insetMm: 10 });
  /** Everything is cut mirrored (heat transfer vinyl is cut from the back). */
  mirrored = $state(false);
  /** The user's own material profiles. */
  customMaterials = $state<Material[]>([]);
  materials = $derived([...BUILTIN_MATERIALS, ...this.customMaterials]);
  /** Installed font families, once asked for. */
  fonts = $state<string[] | null>(null);
  make = $state<MakeState | null>(null);
  /** Click single lines of the selected design to leave them out. */
  lineSelect = $state(false);
  /** Contour path data by design and offset (in the design's own mm). */
  contours = $state<Record<string, string>>({});
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
  scanningBluetooth = $state(false);
  understood = $state(false);

  view = $state<View>({ scale: 2, x: 40, y: 40 });
  viewport = $state({ w: 800, h: 600 });
  fitRequest = $state(0);

  preview = $state<PreviewState | null>(null);
  send = $state<SendState | null>(null);
  test = $state<TestCut | null>(null);
  /** Where test cuts start; the next one goes beside it, away from that corner. */
  testCorner = $state<TestCorner>('tr');
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
  /** Print and cut: where the marks go, `null` when they do not fit. */
  marks = $derived.by(() => {
    if (!this.printMode || !this.mat) return null;
    const paper = PAPERS[this.printSetup.paper];
    const area = { w: this.mat.areaWidthMm, h: this.mat.id === 'none' ? 18000 : this.mat.areaHeightMm };
    return markLayout(paper, area, this.printSetup.insetMm);
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
    const add = (color: string | null, shapes: number, lengthMm: number) => {
      const entry = out.find((e) => e.color === color);
      if (entry) {
        entry.shapes += shapes;
        entry.lengthMm += lengthMm;
      } else out.push({ color, shapes, lengthMm });
    };
    for (const item of this.items) {
      const design = this.designs[item.design];
      if (!design) continue;
      const k = Math.sqrt(Math.abs(item.sx * item.sy));
      if (item.cut.kind === 'none') continue;
      if (item.cut.kind === 'contour') {
        const [x0, y0, x1, y1] = design.bbox;
        add(CONTOUR_COLOR, 1, 2 * (x1 - x0 + y1 - y0) * k);
        continue;
      }
      for (const c of design.colors) add(c.color, c.shapes, c.lengthMm * k);
    }
    for (const item of this.items) {
      const box = this.boxes[item.id];
      if (!item.weed || !box || item.cut.kind === 'none') continue;
      const m = item.weed.marginMm;
      add(WEED_COLOR, 1, 2 * (box.x1 - box.x0 + box.y1 - box.y0 + 4 * m));
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
      if (design)
        items.push({
          design: item.design,
          matrix: placementMatrix(item, design),
          cut: $state.snapshot(item.cut),
          print: this.printMode && item.print,
          weed: item.weed ? { ...item.weed } : null,
        });
    }
    return {
      machine: machine.id,
      mat: mat.id,
      print: this.printMode ? $state.snapshot(this.printSetup) : null,
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
      items: [{ design: design.id, matrix: [1, 0, 0, 1, test.x, test.y], cut: { kind: 'lines', outerOnly: false, hidden: [] }, print: false, weed: null }],
      print: null,
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
    const stored = this.prefs[colorKey(color)];
    if (stored) return stored;
    // Weeding lines go through the same film as the design: its first colour's settings.
    if (color === WEED_COLOR) {
      const film = this.colorInfos.find((c) => c.color !== WEED_COLOR && c.color !== CONTOUR_COLOR && this.prefs[colorKey(c.color)]);
      if (film) return { settings: { ...this.prefs[colorKey(film.color)].settings }, skip: false, pauseBefore: false };
    }
    return { settings: defaultSettings(), skip: false, pauseBefore: false };
  }

  /** The stored prefs of a colour, created on first change. */
  editPrefs(color: string | null): ColorPrefs {
    const key = colorKey(color);
    if (!this.prefs[key]) this.prefs[key] = $state.snapshot(this.prefsOf(color));
    return this.prefs[key];
  }

  /** Puts a material's settings on a colour (or the single pass); heat transfer vinyl mirrors the job. */
  applyMaterial(settings: Settings, material: Material): void {
    Object.assign(settings, $state.snapshot(material.settings));
    if (material.mirror && !this.mirrored && this.items.length) {
      this.setMirror(true);
      this.toast(t('material.mirrored'), 'info');
    }
    this.savePrefs();
  }

  saveMaterial(name: string, settings: Settings): void {
    const trimmed = name.trim();
    if (!trimmed) return;
    const rest = this.customMaterials.filter((m) => m.name !== trimmed);
    this.customMaterials = [...rest, { id: `u:${Date.now()}`, name: trimmed, builtin: false, settings: $state.snapshot(settings) }];
    storeMaterials($state.snapshot(this.customMaterials));
  }

  deleteMaterial(id: string): void {
    this.customMaterials = this.customMaterials.filter((m) => m.id !== id);
    storeMaterials($state.snapshot(this.customMaterials));
  }

  materialName(m: Material): string {
    return m.builtin ? t(m.name as Key) : m.name;
  }

  /** Mirrors every design across the middle of the layout, or back. */
  setMirror(on: boolean): void {
    if (on === this.mirrored) return;
    this.mirrored = on;
    const box = this.itemsBox;
    if (!box || !this.items.length) return;
    this.checkpoint();
    const mid = (box.x0 + box.x1) / 2;
    for (const item of this.items) {
      item.cx = 2 * mid - item.cx;
      item.sx = -item.sx;
      item.angle = normalizeAngle(-item.angle);
    }
  }

  async init(): Promise<void> {
    const stored = loadPrefs();
    if (stored.unit) this.unit = stored.unit;
    if (stored.byColor !== undefined) this.byColor = stored.byColor;
    if (stored.single) this.single = { ...defaultSettings(), ...stored.single };
    if (stored.colors) this.prefs = stored.colors;
    if (stored.print) this.printSetup = { ...this.printSetup, ...stored.print };
    if (stored.testCorner && ['tl', 'tr', 'bl', 'br'].includes(stored.testCorner)) this.testCorner = stored.testCorner;
    this.customMaterials = loadMaterials();
    const [info, machines] = await Promise.all([backend.appInfo(), backend.machines()]);
    this.info = info;
    this.machines = machines;
    const machine = machines.find((m) => m.id === stored.machineId) ?? machines[0];
    this.machineId = machine.id;
    this.matId = machine.mats.some((m) => m.id === stored.matId) ? (stored.matId as string) : machine.defaultMat;
    if (!stored.unit && /^en-(US|LR|MM)/i.test(info.language)) this.unit = 'in';
    void this.checkUsbAccess(true);
  }

  /** Looks for the udev rule; at start the dialog opens once unless the user said not to ask. */
  async checkUsbAccess(atStart = false): Promise<void> {
    try {
      this.usb = await backend.usbAccess();
    } catch {
      this.usb = null;
      return;
    }
    const missing = this.usb.needed && !this.usb.installed;
    let quiet = false;
    try {
      quiet = localStorage.getItem(USB_QUIET_KEY) === '1';
    } catch {
      // No storage: ask every time.
    }
    if (atStart && missing && !quiet) this.usbDialog = true;
  }

  usbNeverAsk(): void {
    try {
      localStorage.setItem(USB_QUIET_KEY, '1');
    } catch {
      // Nothing to remember it in.
    }
    this.usbDialog = false;
  }

  savePrefs(): void {
    const data: StoredPrefs = {
      machineId: this.machineId,
      matId: this.matId,
      unit: this.unit,
      byColor: this.byColor,
      single: $state.snapshot(this.single),
      colors: $state.snapshot(this.prefs),
      print: $state.snapshot(this.printSetup),
      testCorner: this.testCorner,
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
    // Another model starts on its own standard mat (the Portrait 4's is 8.5 × 12 in, not the
    // Portrait 3's 8 × 12 it also takes).
    const changed = id !== this.machineId;
    this.machineId = id;
    if (changed || !machine.mats.some((m) => m.id === this.matId)) this.matId = machine.defaultMat;
    this.status = null;
    this.statusError = null;
    this.fitRequest++;
    this.savePrefs();
  }

  setMat(id: string): void {
    this.matId = id;
    this.fitRequest++;
    this.savePrefs();
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
    if (!design.shapes.length && !design.image) {
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
    // Print and cut: inside the marks, clear of the square at the top left.
    const start = this.marks ? { x: this.marks.left + MARK.square + MARK.clearance + 4, y: this.marks.top + MARK.square + MARK.clearance + 4 } : { x: 10, y: 10 };
    if (!fits || this.marks) {
      cx = Math.min(start.x, Math.max(0, (area.w - w) / 2)) + w / 2;
      cy = start.y + h / 2;
    }
    // Not exactly on top of another item.
    while (this.items.some((i) => Math.abs(i.cx - cx) < 0.5 && Math.abs(i.cy - cy) < 0.5)) {
      cx += 10;
      cy += 10;
    }
    // Pictures larger than most of the cutting area start smaller.
    let k = 1;
    if (design.image) {
      k = Math.min(1, (area.w * 0.8) / w, (area.h * 0.8) / h);
      if (!fits || k < 1 || this.marks) {
        cx = start.x + (w * k) / 2;
        cy = start.y + (h * k) / 2;
      }
    }
    const cut: ItemCut = design.image && this.printMode ? { kind: 'contour', offsetMm: 2 } : { kind: 'lines', outerOnly: false, hidden: [] };
    const item: Item = { id: nextItemId++, design: design.id, cx, cy, angle: 0, sx: this.mirrored ? -k : k, sy: k, cut, print: !!design.printable, weed: null };
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

  /** Changes how the selected item is cut. */
  setCut(cut: ItemCut): void {
    const item = this.selectedItem;
    if (!item) return;
    this.checkpoint();
    item.cut = cut;
    if (cut.kind !== 'lines') this.lineSelect = false;
  }

  /** Leaves a line of the selected item out, or puts it back. */
  toggleLine(index: number): void {
    const item = this.selectedItem;
    if (!item || item.cut.kind !== 'lines') return;
    this.checkpoint();
    const hidden = item.cut.hidden;
    item.cut = { ...item.cut, hidden: hidden.includes(index) ? hidden.filter((i) => i !== index) : [...hidden, index] };
  }

  /** Traces a picture again; every item showing it follows. */
  async retrace(designId: number, trace: TraceRequest): Promise<void> {
    try {
      const info = await backend.traceDesign(designId, trace);
      this.designs[designId] = info;
      for (const key of Object.keys(this.contours)) if (key.startsWith(`${designId}|`)) delete this.contours[key];
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  /** The contour path of an item's design at its offset, in the design's mm; fetched once. */
  contourKey(item: Item): string | null {
    if (item.cut.kind !== 'contour') return null;
    const scale = Math.sqrt(Math.abs(item.sx * item.sy)) || 1;
    return `${item.design}|${(item.cut.offsetMm / scale).toFixed(2)}`;
  }

  async loadContour(key: string): Promise<void> {
    if (key in this.contours) return;
    this.contours[key] = '';
    const [design, offset] = key.split('|');
    try {
      this.contours[key] = await backend.contourPath(Number(design), Number(offset));
    } catch {
      delete this.contours[key];
    }
  }

  /** Saves the print sheet as PDF. */
  async savePrintSheet(): Promise<void> {
    const job = this.job;
    if (!job || !job.print) return;
    try {
      const path = await backend.printSheet(job);
      if (path) this.toast(t('print.saved', { path }), 'info');
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  /** Weeding aids for the selected design, or none. */
  setWeed(weed: WeedRequest | null): void {
    const item = this.selectedItem;
    if (!item) return;
    this.checkpoint();
    item.weed = weed;
  }

  /** The area designs may go in: inside the marks in print and cut, else the cutting area. */
  private placeArea(): Box {
    const m = this.marks;
    if (m) {
      const clear = MARK.bracket + MARK.clearance;
      return { x0: m.left + MARK.square + MARK.clearance, y0: m.top + MARK.square + MARK.clearance, x1: m.right - MARK.clearance, y1: m.bottom - clear };
    }
    // A millimetre from the edges: the exact outline may reach a hair beyond the hull it is measured by.
    return { x0: 1, y0: 1, x1: this.area.w - 1, y1: this.mat?.id === 'none' ? Infinity : this.area.h - 1 };
  }

  /** The box an item takes, its weeding box included. */
  private takenBox(item: Item): Box | null {
    const design = this.designs[item.design];
    if (!design) return null;
    const b = placedBox(item, design);
    const m = item.weed ? item.weed.marginMm : 0;
    return { x0: b.x0 - m, y0: b.y0 - m, x1: b.x1 + m, y1: b.y1 + m };
  }

  /**
   * Arranges all designs to use little material: in rows from the top left, the tallest first,
   * smaller ones filling the gaps rows leave, turned a quarter when that is the only way one fits
   * across.
   */
  arrange(): void {
    if (!this.items.length) return;
    this.checkpoint();
    const area = this.placeArea();
    const gap = ARRANGE_GAP_MM;
    const sized = this.items
      .map((item) => ({ item, box: this.takenBox(item) }))
      .filter((e): e is { item: Item; box: Box } => !!e.box)
      .sort((a, b) => b.box.y1 - b.box.y0 - (a.box.y1 - a.box.y0) || b.box.x1 - b.box.x0 - (a.box.x1 - a.box.x0));
    // First fit: every design goes into the first row with room left, else opens a new row.
    const rows: { y: number; h: number; x: number }[] = [];
    let overflow = false;
    for (const { item } of sized) {
      let box = this.takenBox(item)!;
      if (box.x1 - box.x0 > area.x1 - area.x0) {
        item.angle = normalizeAngle(item.angle + 90);
        box = this.takenBox(item)!;
      }
      const w = box.x1 - box.x0;
      const h = box.y1 - box.y0;
      let row = rows.find((r) => r.x + w <= area.x1 + 1e-6 && h <= r.h + 1e-6);
      if (!row) {
        const last = rows[rows.length - 1];
        row = { y: last ? last.y + last.h + gap : area.y0, h, x: area.x0 };
        rows.push(row);
      }
      if (row.y + h > area.y1) overflow = true;
      item.cx += row.x - box.x0;
      item.cy += row.y - box.y0;
      row.x += w + gap;
    }
    if (overflow) this.toast(t('arrange.overflow'));
  }

  /** A free spot for a box of `w` × `h` mm, scanning rows from the top left; `null` if none. */
  private freeSpot(w: number, h: number, taken: Box[]): { x: number; y: number } | null {
    const area = this.placeArea();
    const gap = ARRANGE_GAP_MM;
    const bottom = Number.isFinite(area.y1) ? area.y1 : area.y0 + 18000;
    for (let y = area.y0; y + h <= bottom; y += 2) {
      for (let x = area.x0; x + w <= area.x1; x += 2) {
        const b = { x0: x - gap, y0: y - gap, x1: x + w + gap, y1: y + h + gap };
        const clash = taken.find((o) => o.x1 > b.x0 && o.x0 < b.x1 && o.y1 > b.y0 && o.y0 < b.y1);
        if (!clash) return { x, y };
        // Jump past the box in the way.
        x = Math.max(x, clash.x1 + gap - 2);
      }
    }
    return null;
  }

  /** Puts `count` copies of the selected design in free spots. */
  addCopies(count: number): void {
    const item = this.selectedItem;
    const box = item ? this.takenBox(item) : null;
    if (!item || !box || count < 1) return;
    this.checkpoint();
    const taken = this.items.map((i) => this.takenBox(i)).filter((b): b is Box => !!b);
    const w = box.x1 - box.x0;
    const h = box.y1 - box.y0;
    let placed = 0;
    for (; placed < Math.min(count, 200); placed++) {
      const spot = this.freeSpot(w, h, taken);
      if (!spot) break;
      const copy: Item = { ...$state.snapshot(item), id: nextItemId++, cx: item.cx + spot.x - box.x0, cy: item.cy + spot.y - box.y0 };
      this.items.push(copy);
      taken.push({ x0: spot.x, y0: spot.y, x1: spot.x + w, y1: spot.y + h });
    }
    if (placed < count) this.toast(t('arrange.copiesLeft', { n: placed }));
  }

  // Shapes and text

  async loadFonts(): Promise<void> {
    if (this.fonts) return;
    try {
      this.fonts = await backend.fonts();
    } catch {
      this.fonts = [];
    }
  }

  openMake(kind: 'text' | 'shape'): void {
    const spec: MadeSpec =
      kind === 'text'
        ? { kind: 'text', text: '', family: '', sizeMm: 25, bold: false, italic: false, align: 'center', letterSpacingMm: 0, lineHeight: 1.2 }
        : { kind: 'shape', shape: 'rectangle', widthMm: 50, heightMm: 50, cornerMm: 0, sides: 5, inner: 0.5 };
    this.make = { spec, itemId: null };
    if (kind === 'text') void this.loadFonts();
  }

  editMade(): void {
    const item = this.selectedItem;
    const made = item ? this.designs[item.design]?.made : undefined;
    if (!item || !made) return;
    this.make = { spec: $state.snapshot(made), itemId: item.id };
    if (made.kind === 'text') void this.loadFonts();
  }

  /** Puts the made design on the mat, or swaps it in for the item being changed. */
  applyMade(design: DesignInfo): void {
    const make = this.make;
    this.make = null;
    if (!make) return;
    const item = make.itemId === null ? null : this.items.find((i) => i.id === make.itemId);
    if (!item) {
      void this.open(async () => [design]);
      return;
    }
    this.checkpoint();
    const old = item.design;
    this.designs[design.id] = design;
    item.design = design.id;
    item.sx = Math.sign(item.sx || 1);
    item.sy = Math.sign(item.sy || 1);
    if (item.cut.kind === 'lines') item.cut = { ...item.cut, hidden: [] };
    if (!this.items.some((i) => i.design === old)) {
      delete this.designs[old];
      void backend.forgetDesign(old);
    }
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
      const usb = await backend.scanDevices();
      const before = this.device;
      this.devices = [...usb, ...this.devices.filter((d) => d.bluetooth)];
      if (this.target !== 'virtual' && !this.devices.some((d) => deviceKey(d) === this.target)) {
        this.target = 'virtual';
        if (before) this.toast(t('machine.lost', { name: before.name }), 'info');
      }
    } catch (e) {
      this.devices = this.devices.filter((d) => d.bluetooth);
      if (!backend.mock) this.toast(errorText(e));
    } finally {
      this.scanning = false;
    }
  }

  /** The connection assistant for Bluetooth: searches, lists what it finds, picks a single machine. */
  async scanBluetooth(): Promise<void> {
    this.scanningBluetooth = true;
    try {
      const found = await backend.scanBluetooth();
      this.devices = [...this.devices.filter((d) => !d.bluetooth), ...found];
      if (found.length === 0) this.toast(t('machine.bluetoothNone'));
      else if (this.target === 'virtual' && found.length === 1) this.target = deviceKey(found[0]);
    } catch (e) {
      this.toast(errorText(e));
    } finally {
      this.scanningBluetooth = false;
    }
  }

  device = $derived(this.devices.find((d) => deviceKey(d) === this.target) ?? null);

  /**
   * Asks the machine how it is. `quiet` (the regular check while a machine is chosen) keeps the
   * last answer on screen until the new one is there.
   */
  async checkStatus(quiet = false): Promise<void> {
    if (quiet && this.checking) return;
    if (!quiet) {
      this.checking = true;
      this.statusError = null;
    }
    const target = this.target;
    try {
      const device = this.device;
      const status = device?.bluetooth
        ? await backend.bluetoothStatus(device.bluetooth)
        : device
          ? await backend.deviceStatus(device.bus, device.address)
          : await backend.virtualStatus(this.machineId);
      // An answer for a connection that is no longer chosen is not shown.
      if (target !== this.target) return;
      this.status = status;
      this.statusError = null;
    } catch (e) {
      if (target !== this.target) return;
      this.status = null;
      this.statusError = e as ApiError;
    } finally {
      if (!quiet) this.checking = false;
    }
  }

  /** A job is under way (or being started): the machine is not asked in between. */
  jobBusy = $derived(!!this.send && ['connecting', 'running', 'paused', 'finishing', 'stopping'].includes(this.send.phase));

  targetValue(): Target {
    const device = this.device;
    if (device?.bluetooth) return { kind: 'bluetooth', address: device.bluetooth };
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

  /**
   * A free 12 mm spot for a test cut: from the chosen corner, 15 mm inside the cutting area, along
   * the edge away from that corner, then the next row towards the middle; clear of designs and
   * earlier tests.
   */
  freeTestSpot(): { x: number; y: number } {
    const size = TEST_SIZE_MM;
    const gap = 4;
    const margin = TEST_MARGIN_MM;
    const area = this.area;
    const right = this.testCorner.endsWith('r');
    const bottom = this.testCorner.startsWith('b');
    const taken: Box[] = [
      ...Object.values(this.boxes),
      ...this.testSpots.map((p) => ({ x0: p.x, y0: p.y, x1: p.x + size, y1: p.y + size })),
    ];
    const columns = Math.max(1, Math.floor((area.w - 2 * margin + gap) / (size + gap)));
    const rows = Math.max(1, Math.floor((area.h - 2 * margin + gap) / (size + gap)));
    for (let j = 0; j < rows; j++) {
      const y = bottom ? area.h - margin - size - j * (size + gap) : margin + j * (size + gap);
      for (let k = 0; k < columns; k++) {
        const x = right ? area.w - margin - size - k * (size + gap) : margin + k * (size + gap);
        const b = { x0: x - 2, y0: y - 2, x1: x + size + 2, y1: y + size + 2 };
        if (taken.every((o) => o.x1 <= b.x0 || o.x0 >= b.x1 || o.y1 <= b.y0 || o.y0 >= b.y1)) return { x, y };
      }
    }
    return { x: right ? area.w - margin - size : margin, y: bottom ? area.h - margin - size : margin };
  }

  /** Test cuts start in another corner from now on; the open test moves there. */
  setTestCorner(corner: TestCorner): void {
    this.testCorner = corner;
    this.savePrefs();
    if (!this.test) return;
    const spot = this.freeTestSpot();
    this.test.x = spot.x;
    this.test.y = spot.y;
    void this.planTest();
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
  return d.bluetooth ? `bt:${d.bluetooth}` : `usb:${d.bus}:${d.address}`;
}

export const app = new AppState();
