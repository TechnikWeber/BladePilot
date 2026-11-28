// Plane geometry for the mat view: placements, matrices, bounding boxes. Millimetres, x right,
// y down, angles in degrees clockwise (as SVG draws them).
import type { DesignInfo, Matrix } from './api/types';

export interface Point {
  x: number;
  y: number;
}

export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Where a design sits on the mat: its bounding box centre at (cx, cy), turned and scaled. */
export interface Placement {
  cx: number;
  cy: number;
  angle: number;
  /** Scale along the design's own axes; negative mirrors. */
  sx: number;
  sy: number;
}

export const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];

export function multiply(m: Matrix, n: Matrix): Matrix {
  const [a1, b1, c1, d1, e1, f1] = m;
  const [a2, b2, c2, d2, e2, f2] = n;
  return [a1 * a2 + c1 * b2, b1 * a2 + d1 * b2, a1 * c2 + c1 * d2, b1 * c2 + d1 * d2, a1 * e2 + c1 * f2 + e1, b1 * e2 + d1 * f2 + f1];
}

export function translate(x: number, y: number): Matrix {
  return [1, 0, 0, 1, x, y];
}

export function rotate(degrees: number): Matrix {
  const r = (degrees * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return [cos, sin, -sin, cos, 0, 0];
}

export function scale(sx: number, sy: number): Matrix {
  return [sx, 0, 0, sy, 0, 0];
}

export function apply(m: Matrix, p: Point): Point {
  return { x: m[0] * p.x + m[2] * p.y + m[4], y: m[1] * p.x + m[3] * p.y + m[5] };
}

export function invert(m: Matrix): Matrix {
  const [a, b, c, d, e, f] = m;
  const det = a * d - b * c || 1e-12;
  return [d / det, -b / det, -c / det, a / det, (c * f - d * e) / det, (b * e - a * f) / det];
}

export function designCenter(design: DesignInfo): Point {
  const [x0, y0, x1, y1] = design.bbox;
  return { x: (x0 + x1) / 2, y: (y0 + y1) / 2 };
}

export function designSize(design: DesignInfo): { w: number; h: number } {
  const [x0, y0, x1, y1] = design.bbox;
  return { w: Math.max(x1 - x0, 1e-6), h: Math.max(y1 - y0, 1e-6) };
}

/** From page mm to mat mm. */
export function placementMatrix(p: Placement, design: DesignInfo): Matrix {
  const c = designCenter(design);
  return multiply(multiply(multiply(translate(p.cx, p.cy), rotate(p.angle)), scale(p.sx, p.sy)), translate(-c.x, -c.y));
}

/** A placement mirrored left to right across the vertical line `x = axis` (heat transfer vinyl). */
export function mirrorMatrix(m: Matrix, axis: number): Matrix {
  const [a, b, c, d, e, f] = m;
  return [-a, b, -c, d, 2 * axis - e, f];
}

/** A box mirrored left to right across `x = axis`. */
export function mirrorBox(box: Box, axis: number): Box {
  return { x0: 2 * axis - box.x1, y0: box.y0, x1: 2 * axis - box.x0, y1: box.y1 };
}

export function boxOf(points: Iterable<Point>): Box | null {
  let box: Box | null = null;
  for (const p of points) {
    if (!box) box = { x0: p.x, y0: p.y, x1: p.x, y1: p.y };
    else {
      box.x0 = Math.min(box.x0, p.x);
      box.y0 = Math.min(box.y0, p.y);
      box.x1 = Math.max(box.x1, p.x);
      box.y1 = Math.max(box.y1, p.y);
    }
  }
  return box;
}

/** The exact axis-aligned box of a placed design on the mat, from its convex hull. */
export function placedBox(p: Placement, design: DesignInfo): Box {
  const m = placementMatrix(p, design);
  const hull = design.hull.length ? design.hull : corners(design);
  return boxOf(hull.map(([x, y]) => apply(m, { x, y }))) ?? { x0: p.cx, y0: p.cy, x1: p.cx, y1: p.cy };
}

function corners(design: DesignInfo): [number, number][] {
  const [x0, y0, x1, y1] = design.bbox;
  return [
    [x0, y0],
    [x1, y0],
    [x1, y1],
    [x0, y1],
  ];
}

/** The placed design's own (turned) box corners on the mat: top-left, top-right, bottom-right, bottom-left. */
export function placedCorners(p: Placement, design: DesignInfo): Point[] {
  const m = placementMatrix(p, design);
  return corners(design).map(([x, y]) => apply(m, { x, y }));
}

/** The convex hull on the mat, as SVG polygon points. */
export function placedHull(p: Placement, design: DesignInfo): Point[] {
  const m = placementMatrix(p, design);
  const hull = design.hull.length >= 3 ? design.hull : corners(design);
  return hull.map(([x, y]) => apply(m, { x, y }));
}

export function union(a: Box | null, b: Box | null): Box | null {
  if (!a) return b;
  if (!b) return a;
  return { x0: Math.min(a.x0, b.x0), y0: Math.min(a.y0, b.y0), x1: Math.max(a.x1, b.x1), y1: Math.max(a.y1, b.y1) };
}

export function inside(box: Box, area: { w: number; h: number }, eps = 1e-6): boolean {
  return box.x0 >= -eps && box.y0 >= -eps && box.x1 <= area.w + eps && box.y1 <= area.h + eps;
}

export function normalizeAngle(degrees: number): number {
  let a = degrees % 360;
  if (a > 180) a -= 360;
  if (a <= -180) a += 360;
  return Math.abs(a) < 1e-9 ? 0 : a;
}

export const MM_PER_INCH = 25.4;

/** A best snap of `values` onto `targets` within `threshold`: the shift to apply and the target hit. */
export function snap1d(values: number[], targets: number[], threshold: number): { shift: number; target: number } | null {
  let best: { shift: number; target: number } | null = null;
  for (const v of values) {
    for (const t of targets) {
      const shift = t - v;
      if (Math.abs(shift) <= threshold && (!best || Math.abs(shift) < Math.abs(best.shift))) best = { shift, target: t };
    }
  }
  return best;
}

/** Grid lines near `from..to` every `step`. */
export function gridLines(from: number, to: number, step: number): number[] {
  const out: number[] = [];
  const start = Math.ceil(from / step) * step;
  for (let v = start; v <= to + 1e-9 && out.length < 2000; v += step) out.push(Math.round(v * 1e6) / 1e6);
  return out;
}

/**
 * Flattens path data of the form the Rust side writes (absolute M, L, Q, C, Z with `x,y` pairs)
 * into polylines, each as flat `[x0, y0, x1, y1, …]`. Curves are split into pieces of about
 * `step` mm.
 */
export function flattenPathData(d: string, step = 0.5): number[][] {
  const tokens = d.match(/[MLQCZmlqcz]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?/g) ?? [];
  const lines: number[][] = [];
  let line: number[] = [];
  let i = 0;
  let cmd = '';
  let x = 0;
  let y = 0;
  let sx = 0;
  let sy = 0;
  const next = () => Number(tokens[i++]);
  const finish = () => {
    if (line.length >= 4) lines.push(line);
    line = [];
  };
  const push = (px: number, py: number) => {
    const n = line.length;
    if (n >= 2 && line[n - 2] === px && line[n - 1] === py) return;
    line.push(px, py);
  };
  while (i < tokens.length) {
    const tk = tokens[i];
    if (/[A-Za-z]/.test(tk)) {
      cmd = tk.toUpperCase();
      i++;
      if (cmd === 'Z') {
        if (line.length) push(sx, sy);
        x = sx;
        y = sy;
        finish();
        continue;
      }
    }
    if (cmd === 'M') {
      finish();
      x = next();
      y = next();
      sx = x;
      sy = y;
      push(x, y);
      cmd = 'L';
    } else if (cmd === 'L') {
      x = next();
      y = next();
      push(x, y);
    } else if (cmd === 'Q') {
      const [cx, cy, ex, ey] = [next(), next(), next(), next()];
      const n = Math.max(2, Math.min(64, Math.ceil((Math.hypot(cx - x, cy - y) + Math.hypot(ex - cx, ey - cy)) / step)));
      for (let k = 1; k <= n; k++) {
        const t = k / n;
        const u = 1 - t;
        push(u * u * x + 2 * u * t * cx + t * t * ex, u * u * y + 2 * u * t * cy + t * t * ey);
      }
      x = ex;
      y = ey;
    } else if (cmd === 'C') {
      const [c1x, c1y, c2x, c2y, ex, ey] = [next(), next(), next(), next(), next(), next()];
      const len = Math.hypot(c1x - x, c1y - y) + Math.hypot(c2x - c1x, c2y - c1y) + Math.hypot(ex - c2x, ey - c2y);
      const n = Math.max(2, Math.min(96, Math.ceil(len / step)));
      for (let k = 1; k <= n; k++) {
        const t = k / n;
        const u = 1 - t;
        push(
          u * u * u * x + 3 * u * u * t * c1x + 3 * u * t * t * c2x + t * t * t * ex,
          u * u * u * y + 3 * u * u * t * c1y + 3 * u * t * t * c2y + t * t * t * ey,
        );
      }
      x = ex;
      y = ey;
    } else {
      i++;
    }
  }
  finish();
  return lines;
}

export function polylineLength(line: number[]): number {
  let len = 0;
  for (let k = 2; k < line.length; k += 2) len += Math.hypot(line[k] - line[k - 2], line[k + 1] - line[k - 1]);
  return len;
}

/** Convex hull of points (monotone chain). */
export function convexHull(points: [number, number][]): [number, number][] {
  const pts = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (pts.length < 3) return pts;
  const cross = (o: [number, number], a: [number, number], b: [number, number]) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower: [number, number][] = [];
  for (const p of pts) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) <= 0) lower.pop();
    lower.push(p);
  }
  const upper: [number, number][] = [];
  for (let k = pts.length - 1; k >= 0; k--) {
    const p = pts[k];
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) <= 0) upper.pop();
    upper.push(p);
  }
  return lower.slice(0, -1).concat(upper.slice(0, -1));
}
