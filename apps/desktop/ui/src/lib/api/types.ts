// What the front end and the Rust side exchange. Mirrors apps/desktop/src-tauri/src/model.rs;
// keep both in step.

export interface AppInfo {
  version: string;
  /** BCP 47 tag of the system language, e.g. `de-DE`. */
  language: string;
  platform: string;
  /** True when the mock backend runs in a plain browser. */
  mock?: boolean;
}

export interface MatInfo {
  /** `8x12`, `12x24`, … or `none`. */
  id: string;
  widthMm: number;
  heightMm: number;
  areaWidthMm: number;
  areaHeightMm: number;
}

export interface MachineInfo {
  id: string;
  name: string;
  cutWidthMm: number;
  maxLengthMm: number;
  holders: number;
  target: boolean;
  registrationMarks: boolean;
  defaultMat: string;
  mats: MatInfo[];
}

export interface ShapeInfo {
  d: string;
  stroke: string | null;
  fill: string | null;
  color: string | null;
}

export interface ColorInfo {
  color: string | null;
  shapes: number;
  lengthMm: number;
}

export interface DesignInfo {
  id: number;
  name: string;
  format: 'svg' | 'studio' | 'studio3';
  pageMm: [number, number] | null;
  /** x0, y0, x1, y1 in page mm. */
  bbox: [number, number, number, number];
  shapes: ShapeInfo[];
  colors: ColorInfo[];
  hull: [number, number][];
  warnings: string[];
}

export type Tool = 'blade' | 'pen';

export interface Settings {
  tool: Tool;
  force: number | null;
  speed: number | null;
  depth: number | null;
  repeats: number;
  overcutMm: number;
  bladeOffsetMm: number;
}

/** SVG `matrix(a b c d e f)` from page mm to mat mm. */
export type Matrix = [number, number, number, number, number, number];

export interface PlacedItem {
  design: number;
  matrix: Matrix;
}

export interface ColorRequest {
  color: string | null;
  settings: Settings;
  skip: boolean;
  pauseBefore: boolean;
}

export interface JobRequest {
  machine: string;
  mat: string;
  items: PlacedItem[];
  byColor: boolean;
  manualOrder: boolean;
  settings: Settings;
  colors: ColorRequest[];
}

export interface PassInfo {
  color: string | null;
  settings: Settings;
  paths: number;
  cutMm: number;
  pauseBefore: boolean;
  toolChange: boolean;
}

export interface PlanInfo {
  passes: PassInfo[];
  cutMm: number;
  travelMm: number;
  bbox: [number, number, number, number] | null;
  conflicts: [number, number][];
  commands: number;
  bytes: number;
}

export interface StrokeInfo {
  cut: boolean;
  pass: number | null;
  /** x0, y0, x1, y1, … in mat mm. */
  points: number[];
}

export interface Simulation {
  plan: PlanInfo;
  strokes: StrokeInfo[];
  cutMm: number;
  travelMm: number;
  warnings: string[];
}

export interface DeviceInfo {
  machine: string | null;
  name: string;
  vendorId: number;
  productId: number;
  serial: string | null;
  bus: string;
  address: number;
}

export interface DeviceStatus {
  version: string | null;
  status: 'ready' | 'moving' | 'unloaded' | 'unknown';
  tools: string[];
}

export type Target = { kind: 'virtual' } | { kind: 'usb'; bus: string; address: number };

export interface ApiError {
  code: string;
  message: string;
}

export type JobEvent =
  | { kind: 'connected'; version: string | null }
  | { kind: 'ready' }
  | { kind: 'pass'; index: number }
  | { kind: 'sending'; sent: number; total: number }
  | { kind: 'searchingMarks' }
  | { kind: 'paused'; beforePass: number | null }
  | { kind: 'resumed' }
  | { kind: 'finishing' }
  | { kind: 'done' }
  | { kind: 'stopped' }
  | { kind: 'failed'; error: ApiError };

export type JobAction = 'pause' | 'resume' | 'stop';

/** Everything the UI asks of the Rust side; the mock backend implements the same. */
export interface Backend {
  readonly mock: boolean;
  appInfo(): Promise<AppInfo>;
  machines(): Promise<MachineInfo[]>;
  /** Design files named on the command line, to open at start. */
  startupFiles(): Promise<string[]>;
  /** Lets the user pick design files and opens them. */
  pickDesigns(): Promise<DesignInfo[]>;
  openPath(path: string): Promise<DesignInfo>;
  openFile(file: File): Promise<DesignInfo>;
  /** The built-in demo design. */
  openDemo(): Promise<DesignInfo>;
  forgetDesign(id: number): Promise<void>;
  planJob(job: JobRequest): Promise<PlanInfo>;
  simulateJob(job: JobRequest): Promise<Simulation>;
  scanDevices(): Promise<DeviceInfo[]>;
  deviceStatus(bus: string, address: number): Promise<DeviceStatus>;
  virtualStatus(machine: string): Promise<DeviceStatus>;
  startJob(job: JobRequest, target: Target, onEvent: (e: JobEvent) => void): Promise<void>;
  controlJob(action: JobAction): Promise<void>;
  /** Calls `onPaths` with the paths of files dropped on the window (Tauri only). */
  onFileDrop(onPaths: (paths: string[]) => void, onHover: (over: boolean) => void): Promise<() => void>;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === 'object' && e !== null && 'code' in e && 'message' in e;
}
