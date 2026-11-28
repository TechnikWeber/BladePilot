// A small stand-in for the Rust planner, for the browser demo: the same rules (pens first, inner
// shapes first, nearest next start, bounds check), simpler numbers. The desktop app never uses it.
import { flattenPathData, polylineLength } from '../../geometry';
import type { ApiError, DesignInfo, JobRequest, MachineInfo, PassInfo, PlanInfo, Settings, Simulation, StrokeInfo } from '../types';

type Line = number[];

/** The colour of contour cuts (as on the Rust side). */
const CONTOUR = '#ff00ff';
/** The colour of weeding boxes and lines (as on the Rust side). */
const WEED = '#00a0a0';

/**
 * A convex hull grown by `r` with round corners, closed: the browser demo's stand-in for the
 * contour the Rust side traces.
 */
export function offsetHull(hull: [number, number][], r: number): Line {
  const n = hull.length;
  if (n < 3) return [];
  let area = 0;
  for (let i = 0; i < n; i++) area += hull[i][0] * hull[(i + 1) % n][1] - hull[(i + 1) % n][0] * hull[i][1];
  const sign = area > 0 ? 1 : -1;
  const normal = (i: number) => {
    const [x1, y1] = hull[i];
    const [x2, y2] = hull[(i + 1) % n];
    const len = Math.hypot(x2 - x1, y2 - y1) || 1;
    return Math.atan2(-sign * (x2 - x1) / len, sign * (y2 - y1) / len);
  };
  const out: Line = [];
  for (let i = 0; i < n; i++) {
    let from = normal((i + n - 1) % n);
    let to = normal(i);
    while (to < from) to += 2 * Math.PI;
    if (to - from > Math.PI * 1.5) [from, to] = [to, from + 2 * Math.PI];
    const steps = Math.max(1, Math.ceil((to - from) / (Math.PI / 12)));
    for (let k = 0; k <= steps; k++) {
      const t = from + ((to - from) * k) / steps;
      out.push(hull[i][0] + r * Math.cos(t), hull[i][1] + r * Math.sin(t));
    }
  }
  out.push(out[0], out[1]);
  return out;
}

interface Pass {
  color: string | null;
  settings: Settings;
  paths: Line[];
  depth: number;
}

export interface MockPlan {
  plan: PlanInfo;
  passes: Pass[];
}

function fail(code: string, message: string): never {
  throw { code, message } satisfies ApiError;
}

function bboxOf(line: Line): [number, number, number, number] {
  let b: [number, number, number, number] = [Infinity, Infinity, -Infinity, -Infinity];
  for (let k = 0; k < line.length; k += 2) b = [Math.min(b[0], line[k]), Math.min(b[1], line[k + 1]), Math.max(b[2], line[k]), Math.max(b[3], line[k + 1])];
  return b;
}

function closed(line: Line): boolean {
  const n = line.length;
  return n > 4 && Math.hypot(line[0] - line[n - 2], line[1] - line[n - 1]) < 1e-6;
}

function contains(poly: Line, x: number, y: number): boolean {
  let inside = false;
  for (let k = 2; k < poly.length; k += 2) {
    const [x1, y1, x2, y2] = [poly[k - 2], poly[k - 1], poly[k], poly[k + 1]];
    if (y1 > y !== y2 > y && x < ((x2 - x1) * (y - y1)) / (y2 - y1) + x1) inside = !inside;
  }
  return inside;
}

function around(outer: Line, inner: Line): boolean {
  if (!closed(outer)) return false;
  const a = bboxOf(outer);
  const b = bboxOf(inner);
  return a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3] && contains(outer, inner[0], inner[1]);
}

function depths(paths: Line[]): number[] {
  return paths.map((p, i) => paths.filter((q, j) => j !== i && around(q, p)).length);
}

function order(paths: Line[]): Line[] {
  const depth = depths(paths);
  const left = paths.map((p, i) => ({ p, d: depth[i], used: false }));
  const out: Line[] = [];
  let x = 0;
  let y = 0;
  for (let level = Math.max(0, ...depth); level >= 0; level--) {
    for (;;) {
      let best = -1;
      let bestD = Infinity;
      let reverse = false;
      left.forEach((e, i) => {
        if (e.used || e.d !== level) return;
        const n = e.p.length;
        const df = Math.hypot(e.p[0] - x, e.p[1] - y);
        const dl = Math.hypot(e.p[n - 2] - x, e.p[n - 1] - y);
        const [d, rev] = !closed(e.p) && dl < df ? [dl, true] : [df, false];
        if (d < bestD) [best, bestD, reverse] = [i, d, rev];
      });
      if (best < 0) break;
      left[best].used = true;
      let p = left[best].p;
      if (reverse) {
        const r: Line = [];
        for (let k = p.length - 2; k >= 0; k -= 2) r.push(p[k], p[k + 1]);
        p = r;
      }
      out.push(p);
      x = p[p.length - 2];
      y = p[p.length - 1];
    }
  }
  return out;
}

export function planMock(job: JobRequest, designs: Map<number, DesignInfo>, machines: MachineInfo[]): MockPlan {
  const machine = machines.find((m) => m.id === job.machine) ?? fail('unknownMachine', `unknown machine ${job.machine}`);
  const mat = machine.mats.find((m) => m.id === job.mat) ?? fail('wrongMat', `the ${machine.name} does not take the ${job.mat} mat`);
  const skip = new Set(job.colors.filter((c) => c.skip).map((c) => c.color));
  const rows = new Map(job.colors.map((c) => [c.color, c]));
  const groups: Pass[] = [];
  const addLines = (key: string | null, lines: Line[]) => {
    let group = groups.find((g) => g.color === key);
    if (!group) {
      const settings = job.byColor ? (rows.get(key)?.settings ?? job.settings) : job.settings;
      group = { color: key, settings, paths: [], depth: 0 };
      groups.push(group);
    }
    group.paths.push(...lines);
  };
  for (const item of job.items) {
    const design = designs.get(item.design) ?? fail('unknownDesign', `no design ${item.design}`);
    const [a, b, c, d, e, f] = item.matrix;
    const place = (l: number[]): Line => {
      const out: Line = [];
      for (let k = 0; k < l.length; k += 2) out.push(a * l[k] + c * l[k + 1] + e, b * l[k] + d * l[k + 1] + f);
      return out;
    };
    const cut = item.cut ?? { kind: 'lines', outerOnly: false, hidden: [] };
    if (cut.kind === 'none') continue;
    let kept: { key: string | null; line: Line }[] = [];
    if (cut.kind === 'contour') {
      const scale = Math.sqrt(Math.abs(a * d - b * c)) || 1;
      if (!skip.has(CONTOUR)) kept.push({ key: job.byColor ? CONTOUR : null, line: place(offsetHull(design.hull, cut.offsetMm / scale)) });
    } else {
      const lines: { key: string | null; line: Line }[] = [];
      design.shapes.forEach((shape, index) => {
        if (skip.has(shape.color) || cut.hidden.includes(index)) return;
        for (const l of flattenPathData(shape.d)) lines.push({ key: job.byColor ? shape.color : null, line: place(l) });
      });
      kept = cut.outerOnly ? lines.filter((x) => !lines.some((y) => y !== x && around(y.line, x.line))) : lines;
    }
    // Weeding: the box only (the desktop app adds the lines through the waste).
    if (item.weed && kept.length && !skip.has(WEED)) {
      let bb: [number, number, number, number] = [Infinity, Infinity, -Infinity, -Infinity];
      for (const { line } of kept) {
        const b = bboxOf(line);
        bb = [Math.min(bb[0], b[0]), Math.min(bb[1], b[1]), Math.max(bb[2], b[2]), Math.max(bb[3], b[3])];
      }
      const m = item.weed.marginMm;
      const [x0, y0, x1, y1] = [bb[0] - m, bb[1] - m, bb[2] + m, bb[3] + m];
      kept.push({ key: job.byColor ? WEED : null, line: [x0, y0, x1, y0, x1, y1, x0, y1, x0, y0] });
    }
    for (const { key, line } of kept) addLines(key, [line]);
  }
  const all = groups.flatMap((g) => g.paths);
  const allDepths = depths(all);
  let at = 0;
  for (const g of groups) {
    g.depth = Math.max(0, ...allDepths.slice(at, at + g.paths.length));
    at += g.paths.length;
    g.paths = order(g.paths);
  }
  let passes = groups.filter((g) => g.paths.length);
  passes.sort((p, q) => Number(p.settings.tool !== 'pen') - Number(q.settings.tool !== 'pen') || q.depth - p.depth);
  if (job.byColor && job.manualOrder) {
    const pos = (c: string | null) => {
      const i = job.colors.findIndex((r) => r.color === c);
      return i < 0 ? Infinity : i;
    };
    passes = [...passes].sort((p, q) => pos(p.color) - pos(q.color));
  }
  if (!passes.length) fail('empty', 'the design has nothing to cut');
  let box: [number, number, number, number] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const p of passes) for (const l of p.paths) {
    const b = bboxOf(l);
    box = [Math.min(box[0], b[0]), Math.min(box[1], b[1]), Math.max(box[2], b[2]), Math.max(box[3], b[3])];
  }
  if (box[0] < -1e-6 || box[1] < -1e-6) fail('beforeOrigin', 'the design starts outside the cutting area');
  if (box[2] > mat.areaWidthMm + 1e-6 || box[3] > mat.areaHeightMm + 1e-6) fail('outOfBounds', 'the design does not fit');
  if (job.print) {
    const paper = job.print.paper === 'letter' ? { w: 215.9, h: 279.4 } : { w: 210, h: 297 };
    const inset = job.print.insetMm;
    const right = Math.min(paper.w, mat.areaWidthMm) - inset;
    const bottom = Math.min(paper.h, mat.areaHeightMm) - inset;
    if (box[0] < inset || box[1] < inset || box[2] > right || box[3] > bottom) fail('outsideMarks', 'the design reaches beyond the registration marks');
    const keepOut = [
      [inset - 3, inset - 3, inset + 8, inset + 8],
      [right - 23, inset - 3, right + 3, inset + 23],
      [inset - 3, bottom - 23, inset + 23, bottom + 3],
    ];
    for (const p of passes) for (const l of p.paths) for (let k = 0; k < l.length; k += 2) {
      if (keepOut.some(([x0, y0, x1, y1]) => l[k] >= x0 && l[k] <= x1 && l[k + 1] >= y0 && l[k + 1] <= y1)) fail('onMarks', 'the design touches a registration mark');
    }
  }
  const pauseColors = new Set(job.colors.filter((c) => c.pauseBefore).map((c) => c.color));
  const infos: PassInfo[] = passes.map((p, i) => {
    const toolChange = i > 0 && p.settings.tool !== passes[i - 1].settings.tool;
    return {
      color: p.color,
      settings: p.settings,
      paths: p.paths.length,
      cutMm: p.paths.reduce((s, l) => s + polylineLength(l), 0) * Math.max(1, p.settings.repeats),
      pauseBefore: i > 0 && (toolChange || (job.byColor && pauseColors.has(p.color))),
      toolChange,
    };
  });
  const conflicts: [number, number][] = [];
  passes.forEach((outer, i) => {
    if (outer.settings.tool === 'pen') return;
    passes.forEach((inner, j) => {
      if (j > i && outer.paths.some((o) => inner.paths.some((p) => around(o, p)))) conflicts.push([i, j]);
    });
  });
  let travel = 0;
  let x = 0;
  let y = 0;
  for (const p of passes) for (const l of p.paths) {
    travel += Math.hypot(l[0] - x, l[1] - y);
    x = l[l.length - 2];
    y = l[l.length - 1];
  }
  const points = passes.reduce((s, p) => s + p.paths.reduce((t, l) => t + l.length / 2, 0) * Math.max(1, p.settings.repeats), 0);
  return {
    passes,
    plan: {
      passes: infos,
      cutMm: infos.reduce((s, p) => s + p.cutMm, 0),
      travelMm: travel,
      bbox: box,
      conflicts,
      commands: Math.round(points + passes.length * 12 + 7),
      bytes: Math.round(points * 10 + passes.length * 80),
    },
  };
}

export function simulateMock(job: JobRequest, designs: Map<number, DesignInfo>, machines: MachineInfo[]): Simulation {
  const { plan, passes } = planMock(job, designs, machines);
  const strokes: StrokeInfo[] = [];
  let x = 0;
  let y = 0;
  let cut = 0;
  let travel = 0;
  passes.forEach((p, i) => {
    for (const l of p.paths) {
      for (let r = 0; r < Math.max(1, p.settings.repeats); r++) {
        const move = Math.hypot(l[0] - x, l[1] - y);
        strokes.push({ cut: false, pass: i, points: [x, y, l[0], l[1]] });
        travel += move;
        strokes.push({ cut: true, pass: i, points: l.map((v) => Math.round(v * 100) / 100) });
        cut += polylineLength(l);
        x = l[l.length - 2];
        y = l[l.length - 1];
      }
    }
  });
  const park = plan.bbox ? plan.bbox[3] : y;
  strokes.push({ cut: false, pass: null, points: [x, y, 0, park] });
  travel += Math.hypot(x, park - y);
  return { plan, strokes, cutMm: cut, travelMm: travel, warnings: [] };
}
