<script lang="ts">
  // The workbench: the cutting mat with its grid, rulers and cutting area, the designs on it, and
  // the handles to move, turn and resize them. Everything inside the SVG is in millimetres.
  import { app, type Item, CONTOUR_COLOR, MARK, PAPERS, WEED_COLOR } from '../lib/state.svelte';
  import { type Point, type Placement, MM_PER_INCH, gridLines, normalizeAngle, placementMatrix, placedHull, snap1d } from '../lib/geometry';
  import { displayColor, isDark } from '../lib/theme.svelte';
  import { t, num } from '../lib/i18n/index.svelte';
  import Icon from './Icon.svelte';
  import PreviewLayer from './PreviewLayer.svelte';
  import { backend } from '../lib/api';

  const MARGIN_SIDE = 12.7;
  const MARGIN_TOP = 25.4;
  const MARGIN_BOTTOM = 12.7;
  const MIN_SCALE = 0.4;
  const MAX_SCALE = 60;

  let width = $state(800);
  let height = $state(600);
  let svgEl: SVGSVGElement | undefined = $state();

  type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';
  type Box4 = { x0: number; y0: number; x1: number; y1: number };
  type Drag =
    | { kind: 'move'; ids: number[]; start: Point; origs: Map<number, Placement>; moved: boolean; box: Box4 }
    | { kind: 'rotate'; id: number; start: number; orig: Placement; moved: boolean }
    | { kind: 'scale'; id: number; handle: Handle; orig: Placement; moved: boolean }
    | { kind: 'grotate'; center: Point; start: number; origs: Map<number, Placement>; moved: boolean }
    | { kind: 'gscale'; anchor: Point; corner: Point; origs: Map<number, Placement>; moved: boolean }
    | { kind: 'band'; start: Point; add: boolean }
    | { kind: 'pan'; start: Point; orig: { x: number; y: number } };

  /** Space held: dragging the background pans instead of selecting. */
  let spaceDown = false;
  let band = $state<Box4 | null>(null);

  let drag: Drag | null = null;
  let guides = $state<{ x: number | null; y: number | null }>({ x: null, y: null });
  let readout = $state<string | null>(null);

  const view = $derived(app.view);
  const sheet = $derived(app.sheet);
  const area = $derived(app.area);
  const roll = $derived(app.mat?.id === 'none');
  const inch = $derived(app.unit === 'in');
  /** The grid and rulers may use inches while measurements stay in millimetres. */
  const gridInch = $derived(app.gridInch);
  const px = $derived(1 / view.scale);

  // The sheet around the usable mat area.
  const sheetRect = $derived(roll ? { x: 0, y: 0, w: sheet.w, h: sheet.h } : { x: -MARGIN_SIDE, y: -MARGIN_TOP, w: sheet.w + 2 * MARGIN_SIDE, h: sheet.h + MARGIN_TOP + MARGIN_BOTTOM });

  $effect(() => {
    app.viewport = { w: width, h: height };
  });

  function fit(): void {
    const r = roll ? { x: -8, y: -8, w: area.w + 16, h: Math.min(sheet.h, Math.max(304.8, (app.itemsBox?.y1 ?? 0) + 20)) + 16 } : sheetRect;
    const pad = 36;
    const scale = Math.max(MIN_SCALE, Math.min(MAX_SCALE, Math.min((width - 2 * pad) / r.w, (height - 2 * pad) / r.h)));
    app.view = { scale, x: (width - r.w * scale) / 2 - r.x * scale, y: (height - r.h * scale) / 2 - r.y * scale };
  }

  // Fit on start, on a new mat or machine, on request, and when the window size changes.
  let lastFit = '';
  $effect(() => {
    const key = `${app.fitRequest}|${width}|${height}|${sheet.w}|${roll ? 'roll' : sheet.h}`;
    if (key !== lastFit && width > 50 && height > 50) {
      lastFit = key;
      fit();
    }
  });

  function toMm(e: PointerEvent | WheelEvent | MouseEvent): Point {
    const rect = svgEl?.getBoundingClientRect();
    const sx = e.clientX - (rect?.left ?? 0);
    const sy = e.clientY - (rect?.top ?? 0);
    return { x: (sx - view.x) / view.scale, y: (sy - view.y) / view.scale };
  }

  /** How much a wheel step zooms; a pinch in Chromium comes as a wheel with Ctrl and small steps. */
  function wheelFactor(e: WheelEvent): number {
    const delta = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    return Math.exp(-delta * (e.ctrlKey && Math.abs(delta) < 50 ? 0.01 : 0.0018));
  }

  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    const rect = svgEl?.getBoundingClientRect();
    app.zoomBy(wheelFactor(e), e.clientX - (rect?.left ?? 0), e.clientY - (rect?.top ?? 0));
  }

  // Only the mat zooms, never the page with its toolbar and panels. The wheel listener has to be
  // able to cancel (Svelte's onwheel is passive); Ctrl with the wheel or a pinch over the panels
  // zooms the mat around its middle.
  $effect(() => {
    const el = svgEl;
    if (!el) return;
    const elsewhere = (e: WheelEvent) => {
      if (!e.ctrlKey || el.contains(e.target as Node)) return;
      e.preventDefault();
      app.zoomBy(wheelFactor(e));
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('wheel', elsewhere, { passive: false });
    return () => {
      el.removeEventListener('wheel', onWheel);
      window.removeEventListener('wheel', elsewhere);
    };
  });

  // A touchpad pinch, which the Linux app takes from the web view: zoom around the fingers, or
  // around the mat's middle when they are over the panels.
  $effect(() => {
    let start = 1;
    let unlisten: (() => void) | null = null;
    let gone = false;
    void backend
      .onPinch((p) => {
        if (p.phase === 'begin') start = app.view.scale;
        if (p.phase !== 'begin' && p.phase !== 'update') return;
        const rect = svgEl?.getBoundingClientRect();
        if (!rect) return;
        const over = p.x >= rect.left && p.x <= rect.right && p.y >= rect.top && p.y <= rect.bottom;
        app.zoomBy((start * p.scale) / app.view.scale, over ? p.x - rect.left : rect.width / 2, over ? p.y - rect.top : rect.height / 2);
      })
      .then((off) => {
        if (gone) off();
        else unlisten = off;
      });
    return () => {
      gone = true;
      unlisten?.();
    };
  });

  // Grid and rulers, in the chosen unit.
  const grid = $derived.by(() => {
    const unitMm = gridInch ? MM_PER_INCH : 10;
    const major = gridInch ? MM_PER_INCH : 50;
    const minorStep = gridInch ? (view.scale > 3.2 ? MM_PER_INCH / 4 : MM_PER_INCH / 2) : view.scale > 5 ? 5 : 10;
    const minor = gridLines(0, sheet.w, minorStep).filter((v) => v > 0 && v < sheet.w);
    const minorY = gridLines(0, sheet.h, minorStep).filter((v) => v > 0 && v < sheet.h);
    const isMajor = (v: number) => Math.abs(v / major - Math.round(v / major)) < 1e-6;
    return {
      unitMm,
      major,
      xs: minor.map((v) => ({ v, major: isMajor(v) || (!gridInch && Math.abs(v / unitMm - Math.round(v / unitMm)) < 1e-6 && minorStep < unitMm) })),
      ys: minorY.map((v) => ({ v, major: isMajor(v) || (!gridInch && Math.abs(v / unitMm - Math.round(v / unitMm)) < 1e-6 && minorStep < unitMm) })),
    };
  });

  const ticks = $derived.by(() => {
    const step = gridInch ? MM_PER_INCH / 8 : view.scale > 2.2 ? 1 : 5;
    const kind = (v: number) => {
      if (gridInch) {
        const n = Math.round(v / (MM_PER_INCH / 8));
        return n % 8 === 0 ? 3 : n % 4 === 0 ? 2 : n % 2 === 0 ? 1.4 : 0.9;
      }
      const n = Math.round(v);
      return n % 50 === 0 ? 3 : n % 10 === 0 ? 2.2 : n % 5 === 0 ? 1.4 : 0.8;
    };
    const along = (len: number) => gridLines(0, len, step).map((v) => ({ v, len: kind(v) }));
    const label = gridInch ? MM_PER_INCH : view.scale > 4 ? 10 : 50;
    const labels = (len: number) => gridLines(label, len - 0.01, label).map((v) => ({ v, text: gridInch ? String(Math.round(v / MM_PER_INCH)) : String(Math.round(v)) }));
    return { x: along(sheet.w), y: along(Math.min(sheet.h, 6000)), lx: labels(sheet.w), ly: labels(Math.min(sheet.h, 6000)) };
  });

  function designOf(item: Item) {
    return app.designs[item.design];
  }

  function matrixOf(item: Item): string {
    const d = designOf(item);
    return d ? placementMatrix(item, d).join(' ') : '1 0 0 1 0 0';
  }

  function hullOf(item: Item): string {
    const d = designOf(item);
    return d ? placedHull(item, d).map((p) => `${p.x},${p.y}`).join(' ') : '';
  }

  const skipped = $derived(new Set(app.rows.filter((r) => r.prefs.skip).map((r) => r.color)));

  // Heat transfer vinyl: the mat shows the designs as they read, the preview as the machine cuts
  // them (mirrored across the middle of the layout).
  const shownMirrored = $derived(app.preview && app.mirrored && app.mirrorAxis !== null ? `matrix(-1 0 0 1 ${2 * app.mirrorAxis} 0)` : null);

  // Contours come from the Rust side once per design and offset.
  $effect(() => {
    for (const item of app.items) {
      const key = app.contourKey(item);
      if (key && !(key in app.contours)) void app.loadContour(key);
    }
  });

  // Selection frame: the item's own box, turned with it.
  const frame = $derived.by(() => {
    const item = app.selectedItem;
    if (item?.locked) return null;
    const d = item ? designOf(item) : null;
    if (!item || !d) return null;
    const [x0, y0, x1, y1] = d.bbox;
    const hw = (Math.abs(item.sx) * Math.max(x1 - x0, 1e-6)) / 2;
    const hh = (Math.abs(item.sy) * Math.max(y1 - y0, 1e-6)) / 2;
    const r = (item.angle * Math.PI) / 180;
    const cos = Math.cos(r);
    const sin = Math.sin(r);
    const at = (u: number, v: number): Point => ({ x: item.cx + u * cos - v * sin, y: item.cy + u * sin + v * cos });
    const handles: { h: Handle; p: Point; cursor: string }[] = [
      { h: 'nw', p: at(-hw, -hh), cursor: 'nwse' },
      { h: 'n', p: at(0, -hh), cursor: 'ns' },
      { h: 'ne', p: at(hw, -hh), cursor: 'nesw' },
      { h: 'e', p: at(hw, 0), cursor: 'ew' },
      { h: 'se', p: at(hw, hh), cursor: 'nwse' },
      { h: 's', p: at(0, hh), cursor: 'ns' },
      { h: 'sw', p: at(-hw, hh), cursor: 'nesw' },
      { h: 'w', p: at(-hw, 0), cursor: 'ew' },
    ];
    const rot = at(0, -hh - 26 * px);
    return { item, hw, hh, cos, sin, handles, rot, top: at(0, -hh), corners: [handles[0].p, handles[2].p, handles[4].p, handles[6].p] };
  });

  /** Several items: one frame around them all, with corner handles and the rotate handle. */
  const groupFrame = $derived.by(() => {
    if (app.selectedItems.length < 2) return null;
    const b = app.selectionBox;
    if (!b) return null;
    const corners: { h: 'nw' | 'ne' | 'se' | 'sw'; p: Point; anchor: Point; cursor: string }[] = [
      { h: 'nw', p: { x: b.x0, y: b.y0 }, anchor: { x: b.x1, y: b.y1 }, cursor: 'nwse-resize' },
      { h: 'ne', p: { x: b.x1, y: b.y0 }, anchor: { x: b.x0, y: b.y1 }, cursor: 'nesw-resize' },
      { h: 'se', p: { x: b.x1, y: b.y1 }, anchor: { x: b.x0, y: b.y0 }, cursor: 'nwse-resize' },
      { h: 'sw', p: { x: b.x0, y: b.y1 }, anchor: { x: b.x1, y: b.y0 }, cursor: 'nesw-resize' },
    ];
    const locked = app.selectedItems.some((i) => i.locked);
    return { box: b, corners, rot: { x: (b.x0 + b.x1) / 2, y: b.y0 - 26 * px }, top: { x: (b.x0 + b.x1) / 2, y: b.y0 }, locked };
  });

  function origsOf(items: Item[]): Map<number, Placement> {
    return new Map(items.filter((i) => !i.locked).map((i) => [i.id, { ...i }]));
  }

  function onGroupHandleDown(e: PointerEvent, handle: 'rotate' | { anchor: Point; corner: Point }): void {
    if (e.button !== 0 || !groupFrame) return;
    e.stopPropagation();
    const origs = origsOf(app.selectedItems);
    if (handle === 'rotate') {
      const b = groupFrame.box;
      const center = { x: (b.x0 + b.x1) / 2, y: (b.y0 + b.y1) / 2 };
      const p = toMm(e);
      drag = { kind: 'grotate', center, start: Math.atan2(p.y - center.y, p.x - center.x), origs, moved: false };
    } else {
      drag = { kind: 'gscale', anchor: handle.anchor, corner: handle.corner, origs, moved: false };
    }
    svgEl?.setPointerCapture(e.pointerId);
  }

  function cursorFor(base: string, angle: number): string {
    // Turn the resize cursor with the item, in 45° steps.
    const order = ['ns', 'nesw', 'ew', 'nwse'];
    const i = order.indexOf(base);
    const steps = Math.round(normalizeAngle(angle) / 45);
    return `${order[(((i + steps) % 4) + 4) % 4]}-resize`;
  }

  function onItemDown(e: PointerEvent, item: Item): void {
    if (e.button !== 0 || app.preview) return;
    e.stopPropagation();
    // Shift or Ctrl: add to the selection or take out again.
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      app.toggleSelect(item.id);
      return;
    }
    if (!app.selection.includes(item.id)) app.selected = item.id;
    const box = app.selectionBox;
    const items = app.selectedItems;
    if (!box || !items.some((i) => !i.locked)) return;
    drag = { kind: 'move', ids: items.map((i) => i.id), start: toMm(e), origs: origsOf(items), moved: false, box: { ...box } };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function onHandleDown(e: PointerEvent, handle: Handle | 'rotate'): void {
    if (e.button !== 0) return;
    e.stopPropagation();
    const item = app.selectedItem;
    if (!item) return;
    if (handle === 'rotate') {
      const p = toMm(e);
      drag = { kind: 'rotate', id: item.id, start: Math.atan2(p.y - item.cy, p.x - item.cx), orig: { ...item }, moved: false };
    } else {
      drag = { kind: 'scale', id: item.id, handle, orig: { ...item }, moved: false };
    }
    svgEl?.setPointerCapture(e.pointerId);
  }

  function onBackgroundDown(e: PointerEvent): void {
    // Middle button, Space or Alt: pan. Left button: draw a box to select (Shift adds to it).
    if (e.button === 1 || (e.button === 0 && (spaceDown || e.altKey || app.preview))) {
      drag = { kind: 'pan', start: { x: e.clientX, y: e.clientY }, orig: { x: view.x, y: view.y } };
      svgEl?.setPointerCapture(e.pointerId);
    } else if (e.button === 0) {
      drag = { kind: 'band', start: toMm(e), add: e.shiftKey || e.ctrlKey || e.metaKey };
      svgEl?.setPointerCapture(e.pointerId);
    }
  }

  function begin(d: { moved: boolean }): void {
    if (!d.moved) {
      d.moved = true;
      app.checkpoint();
      app.dragging = true;
    }
  }

  function snapTargets(axis: 'x' | 'y', exclude: number | number[]): number[] {
    const skip = Array.isArray(exclude) ? exclude : [exclude];
    const len = axis === 'x' ? area.w : area.h;
    const step = gridInch ? MM_PER_INCH / 4 : 5;
    const out = [0, len / 2, len, ...gridLines(0, len, step)];
    for (const item of app.items) {
      if (skip.includes(item.id)) continue;
      const b = app.boxes[item.id];
      if (!b) continue;
      if (axis === 'x') out.push(b.x0, (b.x0 + b.x1) / 2, b.x1);
      else out.push(b.y0, (b.y0 + b.y1) / 2, b.y1);
    }
    return out;
  }

  function onMove(e: PointerEvent): void {
    if (!drag) return;
    if (drag.kind === 'pan') {
      app.view = { scale: view.scale, x: drag.orig.x + e.clientX - drag.start.x, y: drag.orig.y + e.clientY - drag.start.y };
      return;
    }
    const p = toMm(e);
    const d = drag;
    if (d.kind === 'band') {
      band = { x0: Math.min(d.start.x, p.x), y0: Math.min(d.start.y, p.y), x1: Math.max(d.start.x, p.x), y1: Math.max(d.start.y, p.y) };
      return;
    }
    if (d.kind === 'move') {
      let dx = p.x - d.start.x;
      let dy = p.y - d.start.y;
      if (!d.moved && Math.hypot(dx, dy) * view.scale < 3) return;
      begin(d);
      const reach = 7 * px;
      let gx: number | null = null;
      let gy: number | null = null;
      if (!e.altKey) {
        const b = d.box;
        const sx = snap1d([b.x0 + dx, (b.x0 + b.x1) / 2 + dx, b.x1 + dx], snapTargets('x', d.ids), reach);
        const sy = snap1d([b.y0 + dy, (b.y0 + b.y1) / 2 + dy, b.y1 + dy], snapTargets('y', d.ids), reach);
        if (sx) {
          dx += sx.shift;
          gx = sx.target;
        }
        if (sy) {
          dy += sy.shift;
          gy = sy.target;
        }
      }
      if (e.shiftKey) {
        if (Math.abs(dx) > Math.abs(dy)) dy = 0;
        else dx = 0;
      }
      for (const item of app.items) {
        const o = d.origs.get(item.id);
        if (!o) continue;
        item.cx = o.cx + dx;
        item.cy = o.cy + dy;
      }
      guides = { x: gx, y: gy };
      readout = `${fmt(d.box.x0 + dx)}, ${fmt(d.box.y0 + dy)}`;
      return;
    }
    if (d.kind === 'grotate') {
      begin(d);
      let delta = ((Math.atan2(p.y - d.center.y, p.x - d.center.x) - d.start) * 180) / Math.PI;
      if (e.shiftKey) delta = Math.round(delta / 15) * 15;
      const r = (delta * Math.PI) / 180;
      for (const item of app.items) {
        const o = d.origs.get(item.id);
        if (!o) continue;
        const vx = o.cx - d.center.x;
        const vy = o.cy - d.center.y;
        item.cx = d.center.x + vx * Math.cos(r) - vy * Math.sin(r);
        item.cy = d.center.y + vx * Math.sin(r) + vy * Math.cos(r);
        item.angle = normalizeAngle(o.angle + delta);
      }
      readout = `${num(normalizeAngle(delta), 1)}°`;
      return;
    }
    if (d.kind === 'gscale') {
      begin(d);
      // Proportional, about the opposite corner, measured along the diagonal.
      const vx = d.corner.x - d.anchor.x;
      const vy = d.corner.y - d.anchor.y;
      const len2 = vx * vx + vy * vy || 1;
      const k = Math.max(0.05, ((p.x - d.anchor.x) * vx + (p.y - d.anchor.y) * vy) / len2);
      for (const item of app.items) {
        const o = d.origs.get(item.id);
        if (!o) continue;
        item.cx = d.anchor.x + (o.cx - d.anchor.x) * k;
        item.cy = d.anchor.y + (o.cy - d.anchor.y) * k;
        item.sx = o.sx * k;
        item.sy = o.sy * k;
      }
      readout = `${num(k * 100, 0)} %`;
      return;
    }
    const item = app.items.find((i) => i.id === d.id);
    if (!item) return;
    if (d.kind === 'rotate') {
      begin(d);
      let angle = d.orig.angle + ((Math.atan2(p.y - d.orig.cy, p.x - d.orig.cx) - d.start) * 180) / Math.PI;
      if (e.shiftKey) angle = Math.round(angle / 15) * 15;
      else {
        const near = Math.round(angle / 90) * 90;
        if (Math.abs(angle - near) < 2.5) angle = near;
      }
      item.angle = normalizeAngle(angle);
      readout = `${num(item.angle, 1)}°`;
    } else if (d.kind === 'scale') {
      begin(d);
      scaleTo(item, d.orig, d.handle, p, e.shiftKey);
      const design = designOf(item);
      if (design) {
        const [x0, y0, x1, y1] = design.bbox;
        readout = `${fmt(Math.abs(item.sx) * (x1 - x0))} × ${fmt(Math.abs(item.sy) * (y1 - y0))}`;
      }
    }
  }

  /** Resizes from a handle, the opposite handle staying where it is. Corners keep the proportions unless Shift is held. */
  function scaleTo(item: Item, orig: Placement, handle: Handle, p: Point, free: boolean): void {
    const design = designOf(item);
    if (!design) return;
    const [x0, y0, x1, y1] = design.bbox;
    const w0 = Math.max(x1 - x0, 1e-6);
    const h0 = Math.max(y1 - y0, 1e-6);
    const hw = (Math.abs(orig.sx) * w0) / 2;
    const hh = (Math.abs(orig.sy) * h0) / 2;
    const r = (orig.angle * Math.PI) / 180;
    const cos = Math.cos(r);
    const sin = Math.sin(r);
    // Pointer in the item's turned frame, around its original centre.
    const u = { x: (p.x - orig.cx) * cos + (p.y - orig.cy) * sin, y: -(p.x - orig.cx) * sin + (p.y - orig.cy) * cos };
    const hx = handle.includes('e') ? 1 : handle.includes('w') ? -1 : 0;
    const hy = handle.includes('s') ? 1 : handle.includes('n') ? -1 : 0;
    const a = { x: -hx * hw, y: -hy * hh };
    const h = { x: hx * hw, y: hy * hh };
    const minK = 0.5 / Math.max(hw, hh, 0.5);
    let kx = 1;
    let ky = 1;
    if (hx && hy && !free) {
      const dx = h.x - a.x;
      const dy = h.y - a.y;
      const k = Math.max(minK, ((u.x - a.x) * dx + (u.y - a.y) * dy) / (dx * dx + dy * dy));
      kx = k;
      ky = k;
    } else {
      if (hx) kx = Math.max(0.5 / hw, (u.x - a.x) / (h.x - a.x));
      if (hy) ky = Math.max(0.5 / hh, (u.y - a.y) / (h.y - a.y));
    }
    const c = { x: a.x + ((h.x - a.x) * kx) / 2, y: a.y + ((h.y - a.y) * ky) / 2 };
    if (!hx) c.x = 0;
    if (!hy) c.y = 0;
    item.cx = orig.cx + c.x * cos - c.y * sin;
    item.cy = orig.cy + c.x * sin + c.y * cos;
    item.sx = orig.sx * kx;
    item.sy = orig.sy * ky;
  }

  function onUp(e: PointerEvent): void {
    if (svgEl?.hasPointerCapture(e.pointerId)) svgEl.releasePointerCapture(e.pointerId);
    if (drag?.kind === 'band') {
      const small = !band || (band.x1 - band.x0) * view.scale < 4 && (band.y1 - band.y0) * view.scale < 4;
      if (small) {
        if (!drag.add) app.selected = null;
      } else if (band) app.selectInBox(band, drag.add);
      band = null;
    }
    drag = null;
    guides = { x: null, y: null };
    readout = null;
    app.dragging = false;
  }

  function fmt(mm: number): string {
    return inch ? `${num(mm / MM_PER_INCH, 2)} in` : `${num(mm, 1)} mm`;
  }

  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === ' ' && !(e.target as HTMLElement).closest('input, textarea, select')) spaceDown = true;
  }

  function onKeyUp(e: KeyboardEvent): void {
    if (e.key === ' ') spaceDown = false;
  }

  // Drops in a plain browser (the Tauri window reports dropped paths itself).
  function onDragOver(e: DragEvent): void {
    if (!backend.mock || !e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    app.dropping = true;
  }

  async function onDrop(e: DragEvent): Promise<void> {
    if (!backend.mock) return;
    e.preventDefault();
    app.dropping = false;
    const files = Array.from(e.dataTransfer?.files ?? []);
    for (const file of files) await app.open(async () => [await backend.openFile(file)], file.name);
  }

  const cutLabel = $derived(t('mat.area', { w: fmt(area.w), h: roll ? '∞' : fmt(area.h) }));
  const brand = $derived(roll ? '' : app.mat ? `BLADEPILOT  ·  ${app.mat.id.replace('x', ' × ')} in` : 'BLADEPILOT');
  const dark = $derived(isDark());
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />

<div
  class="stage"
  class:dropping={app.dropping}
  bind:clientWidth={width}
  bind:clientHeight={height}
  ondragover={onDragOver}
  ondragleave={() => (app.dropping = false)}
  ondrop={onDrop}
  role="application"
>
  <svg
    bind:this={svgEl}
    class="canvas"
    {width}
    {height}
    viewBox="{-view.x / view.scale} {-view.y / view.scale} {width / view.scale} {height / view.scale}"
    onpointerdown={onBackgroundDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    role="presentation"
  >
    <defs>
      <linearGradient id="mat-surface" x1="0" y1="0" x2="0.35" y2="1">
        <stop offset="0" stop-color="var(--mat-top)" />
        <stop offset="1" stop-color="var(--mat-bottom)" />
      </linearGradient>
      <linearGradient id="mat-sheen" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#ffffff" stop-opacity={dark ? 0.05 : 0.35} />
        <stop offset="0.45" stop-color="#ffffff" stop-opacity="0" />
      </linearGradient>
    </defs>

    <!-- The mat -->
    <g class="mat" style="filter: var(--mat-shadow)">
      <rect x={sheetRect.x} y={sheetRect.y} width={sheetRect.w} height={sheetRect.h} rx={roll ? 0.6 : 5} class={roll ? 'paper' : 'sheet'} />
      {#if !roll}
        <rect x="0" y="0" width={sheet.w} height={sheet.h} fill="url(#mat-surface)" />
      {/if}
    </g>
    {#if !roll}
      <rect x={sheetRect.x} y={sheetRect.y} width={sheetRect.w} height={sheetRect.h} rx="5" fill="url(#mat-sheen)" pointer-events="none" />
    {/if}
    <g class="grid" pointer-events="none">
      {#each grid.xs as g (g.v)}
        <line x1={g.v} y1="0" x2={g.v} y2={sheet.h} class:major={g.major} />
      {/each}
      {#each grid.ys as g (g.v)}
        <line x1="0" y1={g.v} x2={sheet.w} y2={g.v} class:major={g.major} />
      {/each}
      <rect x="0" y="0" width={sheet.w} height={sheet.h} class="frame" />
    </g>

    <!-- Rulers printed on the mat's margins -->
    {#if !roll}
      <g class="ruler" pointer-events="none">
        {#each ticks.x as tk (tk.v)}
          <line x1={tk.v} y1={-0.4} x2={tk.v} y2={-0.4 - tk.len} />
        {/each}
        {#each ticks.y as tk (tk.v)}
          <line x1={-0.4} y1={tk.v} x2={-0.4 - tk.len} y2={tk.v} />
        {/each}
        {#each ticks.lx as l (l.v)}
          <text x={l.v} y={-5.4} text-anchor="middle" font-size="4">{l.text}</text>
        {/each}
        {#each ticks.ly as l (l.v)}
          <text x={-6.9} y={l.v + 1.4} text-anchor="middle" font-size="4">{l.text}</text>
        {/each}
        <text x={sheet.w / 2} y={-13.4} text-anchor="middle" font-size="3.4" class="brand">{brand}</text>
        <path d="M{sheet.w / 2 - 2.6},{-20.4} l2.6,-2.6 l2.6,2.6 M{sheet.w / 2},{-23} v5" class="feed" />
        <text x={sheet.w + MARGIN_SIDE / 2} y={sheet.h + 7.6} text-anchor="middle" font-size="3.2" class="unit">{gridInch ? 'in' : 'mm'}</text>
      </g>
    {/if}

    <!-- Print and cut: the sheet at the top left of the mat, and the marks printed on it -->
    {#if app.printMode}
      {@const paper = PAPERS[app.printSetup.paper]}
      <g class="paper" pointer-events="none">
        <rect x="0" y="0" width={paper.w} height={paper.h} class="sheet-paper" />
        {#if app.marks}
          {@const m = app.marks}
          <rect x={m.left} y={m.top} width={m.right - m.left} height={m.bottom - m.top} class="mark-area" vector-effect="non-scaling-stroke" />
        {/if}
        <text x={paper.w - 2} y={paper.h - 2.5} font-size="3.2" text-anchor="end" class="paper-label">{app.printSetup.paper === 'a4' ? 'A4' : 'Letter'}</text>
      </g>
    {/if}

    <!-- Cutting area -->
    <rect x="0" y="0" width={area.w} height={area.h} class="area" pointer-events="none" />
    {#if !roll}
      <text x="0" y={sheet.h + 7.6} font-size="3.2" class="area-label" pointer-events="none">{cutLabel}</text>
    {/if}
    <g class="origin" pointer-events="none">
      <circle cx="0" cy="0" r={5 * px} />
      <line x1={-9 * px} y1="0" x2={9 * px} y2="0" />
      <line x1="0" y1={-9 * px} x2="0" y2={9 * px} />
    </g>

    <!-- The designs -->
    <g class="items" class:faded={!!app.preview} transform={shownMirrored ?? undefined}>
      {#each app.items as item (item.id)}
        {@const design = designOf(item)}
        {#if design}
          {@const cut = item.cut}
          {@const picking = app.lineSelect && app.selected === item.id && cut.kind === 'lines'}
          <g class="item" class:outside={app.outside.has(item.id)}>
            <g transform="matrix({matrixOf(item)})">
              {#if design.image}
                <image
                  href={design.image.href}
                  x="0"
                  y="0"
                  width={design.image.widthMm}
                  height={design.image.heightMm}
                  preserveAspectRatio="none"
                  class:dim={!app.printMode && cut.kind !== 'none'}
                />
              {/if}
              {#each design.shapes as shape, i (i)}
                <path
                  d={shape.d}
                  fill={shape.fill ? displayColor(shape.fill) : 'none'}
                  fill-opacity="0.13"
                  stroke={displayColor(shape.color)}
                  class:skip={skipped.has(shape.color)}
                  class:off={cut.kind === 'lines' ? cut.hidden.includes(i) : !(app.printMode && item.print)}
                  vector-effect="non-scaling-stroke"
                />
              {/each}
              {#if cut.kind === 'contour'}
                {@const key = app.contourKey(item)}
                {#if key && app.contours[key]}
                  <path d={app.contours[key]} class="contour" class:skip={skipped.has(CONTOUR_COLOR)} vector-effect="non-scaling-stroke" />
                {/if}
              {/if}
              {#if picking}
                {#each design.shapes as shape, i (i)}
                  <path
                    d={shape.d}
                    class="pick"
                    vector-effect="non-scaling-stroke"
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      app.toggleLine(i);
                    }}
                    role="presentation"
                  />
                {/each}
              {/if}
            </g>
            {#if item.weed && app.boxes[item.id] && cut.kind !== 'none'}
              {@const b = app.boxes[item.id]}
              {@const m = item.weed.marginMm}
              <rect
                x={b.x0 - m}
                y={b.y0 - m}
                width={b.x1 - b.x0 + 2 * m}
                height={b.y1 - b.y0 + 2 * m}
                class="weed"
                class:skip={skipped.has(WEED_COLOR)}
                vector-effect="non-scaling-stroke"
                pointer-events="none"
              />
            {/if}
            {#if !picking}
              <polygon points={hullOf(item)} class="hit" vector-effect="non-scaling-stroke" onpointerdown={(e) => onItemDown(e, item)} role="presentation" />
            {/if}
          </g>
        {/if}
      {/each}
    </g>

    <!-- The printed marks, above the designs: nothing may cover them -->
    {#if app.printMode && app.marks}
      {@const m = app.marks}
      <g pointer-events="none">
        <rect x={m.left} y={m.top} width={MARK.square} height={MARK.square} class="mark" />
        <rect x={m.right - MARK.bracket} y={m.top} width={MARK.bracket} height={MARK.line} class="mark" />
        <rect x={m.right - MARK.line} y={m.top} width={MARK.line} height={MARK.bracket} class="mark" />
        <rect x={m.left} y={m.bottom - MARK.bracket} width={MARK.line} height={MARK.bracket} class="mark" />
        <rect x={m.left} y={m.bottom - MARK.line} width={MARK.bracket} height={MARK.line} class="mark" />
      </g>
    {/if}

    {#if app.preview}
      <PreviewLayer {px} />
    {/if}

    <!-- Where the test cut goes -->
    {#if app.send?.mode === 'test' && app.test && app.testDesign}
      <g class="test-marker" transform="translate({app.test.x} {app.test.y})" pointer-events="none">
        <rect x="-2.5" y="-2.5" width="17" height="17" rx="1.6" class="halo" vector-effect="non-scaling-stroke" />
        {#each app.testDesign.shapes as shape, i (i)}
          <path d={shape.d} vector-effect="non-scaling-stroke" />
        {/each}
      </g>
    {/if}

    <!-- Snap guides -->
    {#if guides.x !== null}
      <line x1={guides.x} y1={-MARGIN_TOP} x2={guides.x} y2={area.h + MARGIN_BOTTOM} class="guide" />
    {/if}
    {#if guides.y !== null}
      <line x1={-MARGIN_SIDE} y1={guides.y} x2={area.w + MARGIN_SIDE} y2={guides.y} class="guide" />
    {/if}

    <!-- Selection -->
    {#if frame && !app.preview}
      <g class="selection">
        <polygon points={frame.corners.map((p) => `${p.x},${p.y}`).join(' ')} class="box" vector-effect="non-scaling-stroke" />
        <line x1={frame.top.x} y1={frame.top.y} x2={frame.rot.x} y2={frame.rot.y} class="stem" vector-effect="non-scaling-stroke" />
        <circle
          cx={frame.rot.x}
          cy={frame.rot.y}
          r={5.5 * px}
          class="rotate"
          vector-effect="non-scaling-stroke"
          onpointerdown={(e) => onHandleDown(e, 'rotate')}
          role="presentation"
        />
        {#each frame.handles as h (h.h)}
          <rect
            x={h.p.x - 4.5 * px}
            y={h.p.y - 4.5 * px}
            width={9 * px}
            height={9 * px}
            rx={2 * px}
            class="handle"
            style="cursor: {cursorFor(h.cursor, frame.item.angle)}"
            transform="rotate({frame.item.angle} {h.p.x} {h.p.y})"
            vector-effect="non-scaling-stroke"
            onpointerdown={(e) => onHandleDown(e, h.h)}
            role="presentation"
          />
        {/each}
      </g>
    {/if}
    {#if groupFrame && !app.preview}
      {@const b = groupFrame.box}
      <g class="selection group">
        <rect x={b.x0} y={b.y0} width={b.x1 - b.x0} height={b.y1 - b.y0} class="box multi" vector-effect="non-scaling-stroke" />
        {#each app.selectedItems as item (item.id)}
          {@const ib = app.boxes[item.id]}
          {#if ib}<rect x={ib.x0} y={ib.y0} width={ib.x1 - ib.x0} height={ib.y1 - ib.y0} class="box member" vector-effect="non-scaling-stroke" />{/if}
        {/each}
        {#if !groupFrame.locked}
          <line x1={groupFrame.top.x} y1={groupFrame.top.y} x2={groupFrame.rot.x} y2={groupFrame.rot.y} class="stem" vector-effect="non-scaling-stroke" />
          <circle cx={groupFrame.rot.x} cy={groupFrame.rot.y} r={5.5 * px} class="rotate" vector-effect="non-scaling-stroke" onpointerdown={(e) => onGroupHandleDown(e, 'rotate')} role="presentation" />
          {#each groupFrame.corners as c (c.h)}
            <rect
              x={c.p.x - 4.5 * px}
              y={c.p.y - 4.5 * px}
              width={9 * px}
              height={9 * px}
              rx={2 * px}
              class="handle"
              style="cursor: {c.cursor}"
              vector-effect="non-scaling-stroke"
              onpointerdown={(e) => onGroupHandleDown(e, { anchor: c.anchor, corner: c.p })}
              role="presentation"
            />
          {/each}
        {/if}
      </g>
    {/if}
    {#if app.selectedItem?.locked && app.boxes[app.selectedItem.id]}
      {@const lb = app.boxes[app.selectedItem.id]}
      <rect x={lb.x0} y={lb.y0} width={lb.x1 - lb.x0} height={lb.y1 - lb.y0} class="locked-box" vector-effect="non-scaling-stroke" pointer-events="none" />
    {/if}
    {#if band}
      <rect x={band.x0} y={band.y0} width={band.x1 - band.x0} height={band.y1 - band.y0} class="band" vector-effect="non-scaling-stroke" pointer-events="none" />
    {/if}
    {#if (app.send?.mode === 'align' || app.send?.mode === 'calibrate') && app.check}
      <g class="check-overlay" pointer-events="none">
        {#each app.check.design.shapes as shape, i (i)}
          <path d={shape.d} vector-effect="non-scaling-stroke" />
        {/each}
      </g>
    {/if}
  </svg>

  {#if readout}
    <div class="readout tnum">{readout}</div>
  {/if}

  {#if app.mirrored && app.items.length && !app.send}
    <div class="mirror-chip" class:on={!!app.preview} title={t('colors.mirrorHint')}>
      <Icon name="mirrorH" size={14} />
      <span>{app.preview ? t('mirror.previewOn') : t('mirror.chip')}</span>
      {#if !app.preview && app.plan && !app.planError}
        <button class="chip-btn" onclick={() => app.openPreview()}>{t('mirror.show')}</button>
      {/if}
    </div>
  {/if}

  {#if !app.items.length && app.machines.length && !app.send && !app.make}
    <div class="empty">
      <div class="empty-card">
        <h2>{t('empty.title')}</h2>
        <p>{t('empty.body')}</p>
        <div class="tiles">
          <button class="tile" onclick={() => app.openMake('text')}><Icon name="text" size={22} /><strong>{t('empty.text')}</strong><span>{t('empty.textHint')}</span></button>
          <button class="tile" onclick={() => app.openMake('shape')}><Icon name="shapes" size={22} /><strong>{t('empty.shape')}</strong><span>{t('empty.shapeHint')}</span></button>
          <button class="tile" onclick={() => app.open(() => backend.pickDesigns())}><Icon name="open" size={22} /><strong>{t('empty.open')}</strong><span>{t('empty.openHint')}</span></button>
        </div>
        <div class="empty-actions">
          {#if app.lastSession}
            <button class="btn primary" onclick={() => app.resumeLastSession()}><Icon name="restart" size={15} />{t('empty.resume')}</button>
          {/if}
          {#if app.recent.length}
            <button class="btn" onclick={() => app.openProjectDialog()}><Icon name="folder" size={15} />{t('menu.openProject')}</button>
          {/if}
          <button class="btn ghost" onclick={() => app.openDemo()}><Icon name="sparkle" size={15} />{t('empty.demo')}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if app.dropping}
    <div class="drop"><div><Icon name="open" size={28} />{t('drop.title')}</div></div>
  {/if}
</div>

<style>
  .stage {
    position: relative;
    overflow: hidden;
    background-color: var(--desk);
    background-image: radial-gradient(var(--desk-dot) 1px, transparent 1.2px);
    background-size: 18px 18px;
    min-width: 0;
    min-height: 0;
  }

  .canvas {
    position: absolute;
    inset: 0;
    display: block;
    touch-action: none;
  }

  .sheet {
    fill: var(--mat-margin);
    stroke: var(--mat-edge);
    stroke-width: 0.35;
  }

  .paper {
    fill: #fbfbf8;
    stroke: var(--mat-edge);
    stroke-width: 0.3;
  }

  .grid line {
    stroke: var(--grid-minor);
    stroke-width: 0.18;
  }

  .grid line.major {
    stroke: var(--grid-major);
    stroke-width: 0.3;
  }

  .grid .frame {
    fill: none;
    stroke: var(--grid-major);
    stroke-width: 0.35;
  }

  .ruler line {
    stroke: var(--mat-ink);
    stroke-width: 0.22;
  }

  .ruler text {
    fill: var(--mat-ink);
    font-family: var(--font);
    font-weight: 600;
    user-select: none;
  }

  .ruler .brand {
    letter-spacing: 0.32px;
    font-weight: 700;
    opacity: 0.8;
  }

  .ruler .unit {
    font-weight: 600;
    opacity: 0.8;
  }

  .ruler .feed {
    fill: none;
    stroke: var(--mat-ink);
    stroke-width: 0.45;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0.8;
  }

  .area {
    fill: none;
    stroke: var(--area-line);
    stroke-width: 0.4;
    stroke-dasharray: 2.2 1.4;
    opacity: 0.85;
  }

  .area-label {
    fill: var(--mat-ink);
    font-family: var(--font);
    font-weight: 600;
  }

  .origin circle,
  .origin line {
    fill: none;
    stroke: var(--area-line);
    stroke-width: 0.3;
  }

  .items path {
    stroke-width: 1.6;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .items path.skip {
    stroke-dasharray: 3 3;
    stroke-opacity: 0.45;
    fill-opacity: 0.04;
  }

  .items path.off {
    stroke-opacity: 0.3;
    stroke-dasharray: 2 3;
  }

  .items .contour {
    fill: none;
    stroke: #d6189d;
    stroke-width: 1.8;
    stroke-linejoin: round;
  }

  .items .weed {
    fill: none;
    stroke: #00a0a0;
    stroke-width: 1.4;
    stroke-dasharray: 6 3;
  }

  .items .weed.skip {
    stroke-opacity: 0.35;
  }

  .items .contour.skip {
    stroke-opacity: 0.4;
    stroke-dasharray: 3 3;
  }

  .items image.dim {
    opacity: 0.55;
  }

  .items .pick {
    fill: none;
    stroke: transparent;
    stroke-width: 10;
    cursor: pointer;
  }

  .items .pick:hover {
    stroke: var(--accent);
    stroke-opacity: 0.35;
  }

  .sheet-paper {
    fill: #ffffff;
    stroke: rgba(0, 0, 0, 0.18);
    stroke-width: 0.25;
    filter: drop-shadow(0 0.6px 1px rgba(0, 0, 0, 0.18));
  }

  .mark {
    fill: #111111;
  }

  .mark-area {
    fill: none;
    stroke: rgba(17, 17, 17, 0.25);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }

  .paper-label {
    fill: rgba(0, 0, 0, 0.35);
    font-family: var(--font);
    font-weight: 600;
  }

  .items.faded {
    opacity: 0.22;
  }

  .item .hit {
    fill: transparent;
    stroke: transparent;
    stroke-width: 12;
    cursor: move;
  }

  .item.outside .hit {
    fill: var(--danger-soft);
    stroke: var(--danger);
    stroke-width: 1.2;
    stroke-dasharray: 4 3;
  }

  .items.faded .hit {
    pointer-events: none;
  }

  .test-marker path {
    fill: none;
    stroke: #ea6a1f;
    stroke-width: 2;
    stroke-linejoin: round;
  }

  .test-marker .halo {
    fill: rgba(234, 106, 31, 0.14);
    stroke: #ea6a1f;
    stroke-width: 1.2;
    stroke-dasharray: 4 3;
    animation: blink 1.6s ease-in-out infinite;
  }

  @keyframes blink {
    50% {
      fill-opacity: 0.35;
    }
  }

  .guide {
    stroke: #e8457a;
    stroke-width: 0.25;
    stroke-dasharray: 1.2 0.8;
    pointer-events: none;
  }

  .selection .box {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.4;
    pointer-events: none;
  }

  .selection .box.multi {
    stroke-dasharray: 5 3;
  }

  .selection .box.member {
    stroke-width: 0.8;
    stroke-opacity: 0.55;
  }

  .locked-box {
    fill: none;
    stroke: var(--text-3);
    stroke-width: 1.2;
    stroke-dasharray: 2 3;
  }

  .band {
    fill: var(--accent);
    fill-opacity: 0.08;
    stroke: var(--accent);
    stroke-width: 1;
    stroke-dasharray: 4 3;
  }

  .check-overlay path {
    fill: none;
    stroke: #c2410c;
    stroke-width: 1;
    stroke-opacity: 0.85;
  }

  .selection .stem {
    stroke: var(--accent);
    stroke-width: 1.2;
  }

  .selection .handle,
  .selection .rotate {
    fill: var(--panel);
    stroke: var(--accent);
    stroke-width: 1.6;
  }

  .selection .rotate {
    cursor: grab;
  }

  .selection .handle:hover,
  .selection .rotate:hover {
    fill: var(--accent);
  }

  .mirror-chip {
    position: absolute;
    left: 50%;
    top: 6px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: calc(100% - 32px);
    padding: 3px 4px 3px 10px;
    border-radius: 999px;
    background: var(--panel);
    border: 1px solid var(--accent-line);
    color: var(--accent);
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    box-shadow: var(--shadow-2);
  }

  .mirror-chip span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mirror-chip.on {
    padding-right: 12px;
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .chip-btn {
    height: 22px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font: inherit;
    font-size: 12px;
    font-weight: 650;
    cursor: pointer;
  }

  .chip-btn:hover {
    background: var(--accent);
    color: var(--accent-text);
  }

  .readout {
    position: absolute;
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    padding: 5px 10px;
    border-radius: 6px;
    background: var(--text);
    color: var(--panel);
    font-size: 12px;
    font-weight: 600;
    pointer-events: none;
    box-shadow: var(--shadow-2);
  }

  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
  }

  .empty-card {
    pointer-events: auto;
    width: min(520px, calc(100% - 32px));
    padding: 26px 26px 22px;
    border-radius: 14px;
    background: var(--panel);
    background: color-mix(in srgb, var(--panel) 92%, transparent);
    backdrop-filter: blur(8px);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-2);
    text-align: center;
  }

  .empty h2 {
    margin: 0 0 6px;
    font-size: 16px;
  }

  .empty p {
    margin: 0 0 18px;
    color: var(--text-2);
  }

  .empty-actions {
    display: flex;
    gap: 8px;
    justify-content: center;
    flex-wrap: wrap;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    margin-bottom: 16px;
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 14px 8px 12px;
    border: 1px solid var(--border);
    border-radius: 11px;
    background: var(--panel-2);
    color: var(--text);
    font: inherit;
    cursor: pointer;
    transition:
      border-color 0.12s,
      background 0.12s;
  }

  .tile:hover {
    border-color: var(--accent-line);
    background: var(--accent-soft);
  }

  .tile :global(.icon) {
    color: var(--accent);
  }

  .tile strong {
    font-size: 13px;
  }

  .tile span {
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.3;
  }

  @media (max-width: 700px) {
    .tiles {
      grid-template-columns: 1fr;
    }
  }

  .drop {
    position: absolute;
    inset: 12px;
    display: grid;
    place-items: center;
    border: 2px dashed var(--accent);
    border-radius: 14px;
    background: var(--accent-soft);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--accent);
    font-weight: 650;
    font-size: 15px;
    pointer-events: none;
  }

  .drop div {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
</style>
