// Shapes for the browser demo, built like the Rust side's `make_shape`: the outline fills a box
// of the asked size, its top-left corner at the origin. Text needs the desktop app's fonts.
import { convexHull, flattenPathData, polylineLength } from '../../geometry';
import type { DesignInfo, MadeSpec } from '../types';

const f = (v: number) => Number(v.toFixed(3));

function polygonPath(points: [number, number][], w: number, h: number): string {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const [x0, y0] = [Math.min(...xs), Math.min(...ys)];
  const [sx, sy] = [w / (Math.max(...xs) - x0), h / (Math.max(...ys) - y0)];
  return points.map(([x, y], i) => `${i ? 'L' : 'M'}${f((x - x0) * sx)},${f((y - y0) * sy)}`).join('') + 'Z';
}

function regular(n: number, inner: number | null): [number, number][] {
  const corners = inner === null ? n : 2 * n;
  return Array.from({ length: corners }, (_, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / corners;
    const r = inner !== null && i % 2 === 1 ? inner : 1;
    return [r * Math.cos(a), r * Math.sin(a)] as [number, number];
  });
}

export function shapePath(spec: Extract<MadeSpec, { kind: 'shape' }>): string {
  const w = Math.min(2000, Math.max(0.5, spec.widthMm));
  const h = Math.min(2000, Math.max(0.5, spec.heightMm));
  switch (spec.shape) {
    case 'rectangle': {
      const r = Math.min(Math.max(0, spec.cornerMm), Math.min(w, h) / 2);
      if (r <= 0) return `M0,0L${f(w)},0L${f(w)},${f(h)}L0,${f(h)}Z`;
      const k = r * 0.5523;
      return (
        `M${f(r)},0L${f(w - r)},0C${f(w - r + k)},0 ${f(w)},${f(r - k)} ${f(w)},${f(r)}L${f(w)},${f(h - r)}` +
        `C${f(w)},${f(h - r + k)} ${f(w - r + k)},${f(h)} ${f(w - r)},${f(h)}L${f(r)},${f(h)}C${f(r - k)},${f(h)} 0,${f(h - r + k)} 0,${f(h - r)}` +
        `L0,${f(r)}C0,${f(r - k)} ${f(r - k)},0 ${f(r)},0Z`
      );
    }
    case 'ellipse': {
      const [rx, ry] = [w / 2, h / 2];
      const [kx, ky] = [rx * 0.5523, ry * 0.5523];
      return (
        `M${f(w)},${f(ry)}C${f(w)},${f(ry + ky)} ${f(rx + kx)},${f(h)} ${f(rx)},${f(h)}C${f(rx - kx)},${f(h)} 0,${f(ry + ky)} 0,${f(ry)}` +
        `C0,${f(ry - ky)} ${f(rx - kx)},0 ${f(rx)},0C${f(rx + kx)},0 ${f(w)},${f(ry - ky)} ${f(w)},${f(ry)}Z`
      );
    }
    case 'polygon':
      return polygonPath(regular(Math.min(64, Math.max(3, spec.sides)), null), w, h);
    case 'star':
      return polygonPath(regular(Math.min(64, Math.max(3, spec.sides)), Math.min(0.95, Math.max(0.1, spec.inner))), w, h);
    case 'heart': {
      const p = (x: number, y: number) => `${f(x * w)},${f(y * h)}`;
      return (
        `M${p(0.5, 0.25)}C${p(0.5, 0.05)} ${p(0.25, 0)} ${p(0.15, 0)}C${p(0.05, 0)} ${p(0, 0.12)} ${p(0, 0.3)}` +
        `C${p(0, 0.55)} ${p(0.3, 0.75)} ${p(0.5, 1)}C${p(0.7, 0.75)} ${p(1, 0.55)} ${p(1, 0.3)}` +
        `C${p(1, 0.12)} ${p(0.95, 0)} ${p(0.85, 0)}C${p(0.75, 0)} ${p(0.5, 0.05)} ${p(0.5, 0.25)}Z`
      );
    }
  }
}

export function madeDesign(spec: MadeSpec): DesignInfo {
  if (spec.kind === 'text') throw { code: 'desktopOnly', message: 'text needs the fonts of the desktop app' };
  const d = shapePath(spec);
  const lines = flattenPathData(d);
  const points = lines.flatMap((l) => Array.from({ length: l.length / 2 }, (_, k) => [l[2 * k], l[2 * k + 1]] as [number, number]));
  const [w, h] = [Math.max(...points.map((p) => p[0])), Math.max(...points.map((p) => p[1]))];
  return {
    id: 0,
    name: spec.shape,
    format: 'made',
    pageMm: [w, h],
    bbox: [0, 0, w, h],
    shapes: [{ d, stroke: '#000000', fill: null, color: '#000000' }],
    colors: [{ color: '#000000', shapes: 1, lengthMm: lines.reduce((a, l) => a + polylineLength(l), 0) }],
    hull: convexHull(points),
    warnings: [],
    printable: true,
    made: spec,
  };
}
