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

export interface UsbAccess {
  needed: boolean;
  installed: boolean;
  command: string;
  canElevate: boolean;
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

export interface TraceRequest {
  source: 'alpha' | 'darkness';
  threshold: number;
  despeckleMm2: number;
  invert: boolean;
}

export interface ImageInfo {
  /** A smaller copy as a data: URL. */
  href: string;
  widthMm: number;
  heightMm: number;
  trace: TraceRequest;
}

export interface DesignInfo {
  id: number;
  name: string;
  format: 'svg' | 'studio' | 'studio3' | 'image' | 'dxf' | 'pdf' | 'made';
  pageMm: [number, number] | null;
  /** x0, y0, x1, y1 in page mm. */
  bbox: [number, number, number, number];
  shapes: ShapeInfo[];
  colors: ColorInfo[];
  hull: [number, number][];
  warnings: string[];
  /** Pictures: what is printed and how it was traced. */
  image?: ImageInfo;
  /** The artwork can be printed (pictures, SVG files). */
  printable?: boolean;
  /** A shape or text made in BladePilot: what it was made from. */
  made?: MadeSpec;
}

export type ShapeName = 'rectangle' | 'ellipse' | 'polygon' | 'star' | 'heart';

/** A shape or text to make. */
export type MadeSpec =
  | { kind: 'shape'; shape: ShapeName; widthMm: number; heightMm: number; cornerMm: number; sides: number; inner: number }
  | {
      kind: 'text';
      text: string;
      family: string;
      sizeMm: number;
      bold: boolean;
      italic: boolean;
      align: 'left' | 'center' | 'right';
      letterSpacingMm: number;
      lineHeight: number;
      /** Weld overlapping letters into one outline (script fonts). */
      weld: boolean;
    };

/** A weeding box around a design, with lines through the waste if asked. */
export interface WeedRequest {
  marginMm: number;
  lines: boolean;
}

/** The tool in the holder: blades the machine sets (AutoBlade) or that are set by hand, or a pen. */
export type Tool = 'autoBlade' | 'ratchet' | 'deepCut' | 'kraft' | 'rotary' | 'pen';

export const TOOLS: Tool[] = ['autoBlade', 'ratchet', 'deepCut', 'kraft', 'rotary', 'pen'];

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

/** What of a design is cut. */
export type ItemCut =
  | { kind: 'lines'; outerOnly: boolean; hidden: number[] }
  | { kind: 'contour'; offsetMm: number }
  | { kind: 'none' };

export interface PlacedItem {
  design: number;
  matrix: Matrix;
  cut: ItemCut;
  /** Print and cut: printed on the sheet. */
  print: boolean;
  /** Weeding aids around the design. */
  weed?: WeedRequest | null;
}

export interface PrintSetup {
  paper: 'a4' | 'letter';
  insetMm: number;
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
  /** Print and cut: the machine looks for the registration marks first. */
  print: PrintSetup | null;
  /** The machine's correction, [right, down] mm. */
  offsetMm?: [number, number] | null;
}

export interface TidyResult {
  design: DesignInfo;
  joined: number;
  removed: number;
}

/** A project to save: the front end's own state and the designs it uses. */
export interface ProjectRequest {
  state: unknown;
  designs: number[];
}

export interface OpenedProject {
  state: unknown;
  designs: { savedId: number; design: DesignInfo }[];
  warnings: string[];
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
  /** Bluetooth address (`00:1B:41:CE:BB:CB`) of a machine found over Bluetooth; null on USB. */
  bluetooth: string | null;
}

export interface DeviceStatus {
  version: string | null;
  status: 'ready' | 'moving' | 'unloaded' | 'unknown';
  tools: string[];
}

export type Target = { kind: 'virtual' } | { kind: 'usb'; bus: string; address: number } | { kind: 'bluetooth'; address: string };

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
  /** Linux: is the udev rule for USB access in place? */
  usbAccess(): Promise<UsbAccess>;
  /** Sets the rule up, asking for the administrator password (pkexec). */
  installUsbAccess(): Promise<UsbAccess>;
  machines(): Promise<MachineInfo[]>;
  /** Design files named on the command line, to open at start. */
  startupFiles(): Promise<string[]>;
  /** Lets the user pick design files and opens them. */
  pickDesigns(): Promise<DesignInfo[]>;
  openPath(path: string): Promise<DesignInfo>;
  openFile(file: File): Promise<DesignInfo>;
  /** The built-in demo design. */
  openDemo(): Promise<DesignInfo>;
  /** The test cut pattern (12 mm square with a triangle inside), its corner at the page origin. */
  testDesign(): Promise<DesignInfo>;
  /** Traces a picture again with other settings. */
  traceDesign(id: number, trace: TraceRequest): Promise<DesignInfo>;
  /** The contour `offsetMm` around a design, as path data in its page mm. */
  contourPath(id: number, offsetMm: number): Promise<string>;
  /** Asks where to save the print sheet and writes it as PDF; the path, or null when cancelled. */
  printSheet(job: JobRequest): Promise<string | null>;
  /** The font families installed on the system. */
  fonts(): Promise<string[]>;
  /** Makes a shape or sets a text as a new design. */
  makeDesign(spec: MadeSpec): Promise<DesignInfo>;
  forgetDesign(id: number): Promise<void>;
  /** The alignment test for a sheet laid into the top-left corner of the grid. */
  alignmentDesign(widthMm: number, heightMm: number): Promise<DesignInfo>;
  /** Welds what the items cut into one new design in mat mm. */
  weldItems(items: PlacedItem[]): Promise<DesignInfo>;
  /** Joins line pieces that meet and leaves out lines drawn twice. */
  tidyDesign(id: number): Promise<TidyResult>;
  /** Asks where to save the print and cut calibration sheet and writes it; the path, or null. */
  calibrationSheet(job: JobRequest): Promise<string | null>;
  /** The crosses the machine draws on the calibration sheet, in mat mm. */
  calibrationDesign(job: JobRequest): Promise<DesignInfo>;
  /** Saves a project; asks where when `path` is null. The path, or null when cancelled. */
  saveProject(project: ProjectRequest, path: string | null): Promise<string | null>;
  /** Asks for a project and opens it; null when cancelled. */
  pickProject(): Promise<{ path: string; project: OpenedProject } | null>;
  openProject(path: string): Promise<OpenedProject>;
  /** Keeps the session for the next start. */
  autosave(project: ProjectRequest): Promise<void>;
  restoreAutosave(): Promise<OpenedProject | null>;
  discardAutosave(): Promise<void>;
  /** A report for bug reports: versions, system, machines, log. */
  diagnostics(): Promise<string>;
  /** Asks where to save a text and writes it; the path, or null when cancelled. */
  saveText(text: string, name: string): Promise<string | null>;
  planJob(job: JobRequest): Promise<PlanInfo>;
  simulateJob(job: JobRequest): Promise<Simulation>;
  scanDevices(): Promise<DeviceInfo[]>;
  deviceStatus(bus: string, address: number): Promise<DeviceStatus>;
  /** Machines in Bluetooth range; takes about ten seconds. */
  scanBluetooth(): Promise<DeviceInfo[]>;
  bluetoothStatus(address: string): Promise<DeviceStatus>;
  virtualStatus(machine: string): Promise<DeviceStatus>;
  startJob(job: JobRequest, target: Target, onEvent: (e: JobEvent) => void): Promise<void>;
  controlJob(action: JobAction): Promise<void>;
  /** Calls `onPaths` with the paths of files dropped on the window (Tauri only). */
  onFileDrop(onPaths: (paths: string[]) => void, onHover: (over: boolean) => void): Promise<() => void>;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === 'object' && e !== null && 'code' in e && 'message' in e;
}
