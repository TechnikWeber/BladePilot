// SVG import for the browser demo: the browser's own SVG engine does the geometry. Curves are
// sampled into polylines; the desktop app imports with usvg instead and keeps the curves.
import { convexHull, polylineLength } from '../../geometry';
import type { ColorInfo, DesignInfo, ShapeInfo } from '../types';

const MM_PER_PX = 25.4 / 96;
const STEP_MM = 0.25;
const SKIP_INSIDE = 'defs, clipPath, mask, symbol, marker, pattern, metadata';

function toMm(value: string | null): number | null {
  if (!value) return null;
  const m = value.trim().match(/^([-+]?\d*\.?\d+)(mm|cm|in|pt|pc|px)?$/);
  if (!m) return null;
  const v = Number(m[1]);
  const unit = m[2] ?? 'px';
  return v * ({ mm: 1, cm: 10, in: 25.4, pt: 25.4 / 72, pc: 25.4 / 6, px: MM_PER_PX } as Record<string, number>)[unit];
}

function hex(css: string): string | null {
  const m = css.match(/^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)(?:[,\s/]+([\d.]+))?\s*\)$/);
  if (!m || (m[4] !== undefined && Number(m[4]) === 0)) return null;
  return '#' + [m[1], m[2], m[3]].map((v) => Number(v).toString(16).padStart(2, '0')).join('');
}

function f(v: number): string {
  return String(Math.round(v * 1000) / 1000);
}

export function importSvg(text: string, name: string): DesignInfo {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
  const root = doc.documentElement;
  if (root.nodeName.toLowerCase() !== 'svg' || doc.getElementsByTagName('parsererror').length) {
    throw { code: 'svg', message: 'not an SVG file' };
  }
  const host = document.createElement('div');
  host.style.cssText = 'position:absolute;left:-10000px;top:0;width:10px;height:10px;overflow:hidden;visibility:hidden';
  const svg = document.importNode(root, true) as unknown as SVGSVGElement;
  const vb = svg.viewBox?.baseVal;
  const widthMm = toMm(svg.getAttribute('width')) ?? (vb && vb.width ? vb.width * MM_PER_PX : 210);
  const heightMm = toMm(svg.getAttribute('height')) ?? (vb && vb.height ? vb.height * MM_PER_PX : 297);
  // Render at the page's own size so that one CSS pixel is 25.4/96 mm.
  svg.setAttribute('width', `${widthMm / MM_PER_PX}px`);
  svg.setAttribute('height', `${heightMm / MM_PER_PX}px`);
  host.appendChild(svg);
  document.body.appendChild(host);
  const shapes: ShapeInfo[] = [];
  const warnings: string[] = [];
  const points: [number, number][] = [];
  const colors: ColorInfo[] = [];
  try {
    if (svg.querySelector('text')) warnings.push('text is not cut in the browser demo');
    if (svg.querySelector('image')) warnings.push('a raster image was skipped: images are not cut');
    for (const el of Array.from(svg.querySelectorAll<SVGGeometryElement>('path, rect, circle, ellipse, line, polyline, polygon'))) {
      if (el.closest(SKIP_INSIDE)) continue;
      const style = getComputedStyle(el);
      if (style.display === 'none' || style.visibility === 'hidden') continue;
      const stroke = hex(style.stroke);
      const fill = hex(style.fill);
      const ctm = el.getCTM();
      if (!ctm) continue;
      const total = el.getTotalLength();
      if (!(total > 0)) continue;
      const lenMm = total * Math.hypot(ctm.a, ctm.b) * MM_PER_PX;
      const n = Math.min(20000, Math.max(8, Math.ceil(lenMm / STEP_MM)));
      const lines: number[][] = [];
      let line: number[] = [];
      let last: { x: number; y: number } | null = null;
      for (let k = 0; k <= n; k++) {
        const p = el.getPointAtLength((total * k) / n);
        const x = (ctm.a * p.x + ctm.c * p.y + ctm.e) * MM_PER_PX;
        const y = (ctm.b * p.x + ctm.d * p.y + ctm.f) * MM_PER_PX;
        // A jump means the next subpath.
        if (last && Math.hypot(x - last.x, y - last.y) > STEP_MM * 4 + (lenMm / n) * 2) {
          if (line.length >= 4) lines.push(line);
          line = [];
        }
        line.push(x, y);
        last = { x, y };
      }
      if (line.length >= 4) lines.push(line);
      if (!lines.length) continue;
      const d = lines
        .map((l) => {
          let s = `M${f(l[0])},${f(l[1])}`;
          for (let k = 2; k < l.length; k += 2) s += `L${f(l[k])},${f(l[k + 1])}`;
          const closed = Math.hypot(l[0] - l[l.length - 2], l[1] - l[l.length - 1]) < STEP_MM;
          return closed ? s + 'Z' : s;
        })
        .join('');
      const color = stroke ?? fill;
      shapes.push({ d, stroke, fill, color });
      for (const l of lines) for (let k = 0; k < l.length; k += 2) points.push([l[k], l[k + 1]]);
      const length = lines.reduce((sum, l) => sum + polylineLength(l), 0);
      const entry = colors.find((c) => c.color === color);
      if (entry) {
        entry.shapes += 1;
        entry.lengthMm += length;
      } else colors.push({ color, shapes: 1, lengthMm: length });
    }
  } finally {
    host.remove();
  }
  let bbox: [number, number, number, number] = [0, 0, 0, 0];
  if (points.length) {
    bbox = [Infinity, Infinity, -Infinity, -Infinity];
    for (const [x, y] of points) bbox = [Math.min(bbox[0], x), Math.min(bbox[1], y), Math.max(bbox[2], x), Math.max(bbox[3], y)];
  }
  return { id: 0, name, format: 'svg', pageMm: [widthMm, heightMm], bbox, shapes, colors, hull: convexHull(points), warnings };
}
