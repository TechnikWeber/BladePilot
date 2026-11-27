<script lang="ts">
  // The workbench: the cutting mat with its grid, rulers and cutting area, the designs on it, and
  // the handles to move, turn and resize them. Everything inside the SVG is in millimetres.
  import { app, type Item } from '../lib/state.svelte';
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
  type Drag =
    | { kind: 'move'; id: number; start: Point; orig: Placement; moved: boolean; box: { x0: number; y0: number; x1: number; y1: number } }
    | { kind: 'rotate'; id: number; start: number; orig: Placement; moved: boolean }
    | { kind: 'scale'; id: number; handle: Handle; orig: Placement; moved: boolean }
    | { kind: 'pan'; start: Point; orig: { x: number; y: number } };

  let drag: Drag | null = null;
  let guides = $state<{ x: number | null; y: number | null }>({ x: null, y: null });
  let readout = $state<string | null>(null);

  const view = $derived(app.view);
  const sheet = $derived(app.sheet);
  const area = $derived(app.area);
  const roll = $derived(app.mat?.id === 'none');
  const inch = $derived(app.unit === 'in');
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

  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    const rect = svgEl?.getBoundingClientRect();
    const delta = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    app.zoomBy(Math.exp(-delta * 0.0018), e.clientX - (rect?.left ?? 0), e.clientY - (rect?.top ?? 0));
  }

  // Grid and rulers, in the chosen unit.
  const grid = $derived.by(() => {
    const unitMm = inch ? MM_PER_INCH : 10;
    const major = inch ? MM_PER_INCH : 50;
    const minorStep = inch ? (view.scale > 3.2 ? MM_PER_INCH / 4 : MM_PER_INCH / 2) : view.scale > 5 ? 5 : 10;
    const minor = gridLines(0, sheet.w, minorStep).filter((v) => v > 0 && v < sheet.w);
    const minorY = gridLines(0, sheet.h, minorStep).filter((v) => v > 0 && v < sheet.h);
    const isMajor = (v: number) => Math.abs(v / major - Math.round(v / major)) < 1e-6;
    return {
      unitMm,
      major,
      xs: minor.map((v) => ({ v, major: isMajor(v) || (!inch && Math.abs(v / unitMm - Math.round(v / unitMm)) < 1e-6 && minorStep < unitMm) })),
      ys: minorY.map((v) => ({ v, major: isMajor(v) || (!inch && Math.abs(v / unitMm - Math.round(v / unitMm)) < 1e-6 && minorStep < unitMm) })),
    };
  });

  const ticks = $derived.by(() => {
    const step = inch ? MM_PER_INCH / 8 : view.scale > 2.2 ? 1 : 5;
    const kind = (v: number) => {
      if (inch) {
        const n = Math.round(v / (MM_PER_INCH / 8));
        return n % 8 === 0 ? 3 : n % 4 === 0 ? 2 : n % 2 === 0 ? 1.4 : 0.9;
      }
      const n = Math.round(v);
      return n % 50 === 0 ? 3 : n % 10 === 0 ? 2.2 : n % 5 === 0 ? 1.4 : 0.8;
    };
    const along = (len: number) => gridLines(0, len, step).map((v) => ({ v, len: kind(v) }));
    const label = inch ? MM_PER_INCH : view.scale > 4 ? 10 : 50;
    const labels = (len: number) => gridLines(label, len - 0.01, label).map((v) => ({ v, text: inch ? String(Math.round(v / MM_PER_INCH)) : String(Math.round(v)) }));
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

  // Selection frame: the item's own box, turned with it.
  const frame = $derived.by(() => {
    const item = app.selectedItem;
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
    app.selected = item.id;
    const box = app.boxes[item.id];
    if (!box) return;
    drag = { kind: 'move', id: item.id, start: toMm(e), orig: { ...item }, moved: false, box: { ...box } };
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
    if (e.button === 0 && !app.preview) app.selected = null;
    if (e.button === 0 || e.button === 1) {
      drag = { kind: 'pan', start: { x: e.clientX, y: e.clientY }, orig: { x: view.x, y: view.y } };
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

  function snapTargets(axis: 'x' | 'y', exclude: number): number[] {
    const len = axis === 'x' ? area.w : area.h;
    const step = inch ? MM_PER_INCH / 4 : 5;
    const out = [0, len / 2, len, ...gridLines(0, len, step)];
    for (const item of app.items) {
      if (item.id === exclude) continue;
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
    const d = drag;
    const item = app.items.find((i) => i.id === d.id);
    if (!item) return;
    const p = toMm(e);
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
        const sx = snap1d([b.x0 + dx, (b.x0 + b.x1) / 2 + dx, b.x1 + dx], snapTargets('x', d.id), reach);
        const sy = snap1d([b.y0 + dy, (b.y0 + b.y1) / 2 + dy, b.y1 + dy], snapTargets('y', d.id), reach);
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
      item.cx = d.orig.cx + dx;
      item.cy = d.orig.cy + dy;
      guides = { x: gx, y: gy };
      const b = app.boxes[item.id];
      if (b) readout = `${fmt(b.x0)}, ${fmt(b.y0)}`;
    } else if (d.kind === 'rotate') {
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
    drag = null;
    guides = { x: null, y: null };
    readout = null;
    app.dragging = false;
  }

  function fmt(mm: number): string {
    return inch ? `${num(mm / MM_PER_INCH, 2)} in` : `${num(mm, 1)} mm`;
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
    onwheel={onWheel}
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
        <text x={sheet.w + MARGIN_SIDE / 2} y={sheet.h + 7.6} text-anchor="middle" font-size="3.2" class="unit">{inch ? 'in' : 'mm'}</text>
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
    <g class="items" class:faded={!!app.preview}>
      {#each app.items as item (item.id)}
        {@const design = designOf(item)}
        {#if design}
          <g class="item" class:outside={app.outside.has(item.id)}>
            <g transform="matrix({matrixOf(item)})">
              {#each design.shapes as shape, i (i)}
                <path
                  d={shape.d}
                  fill={shape.fill ? displayColor(shape.fill) : 'none'}
                  fill-opacity="0.13"
                  stroke={displayColor(shape.color)}
                  class:skip={skipped.has(shape.color)}
                  vector-effect="non-scaling-stroke"
                />
              {/each}
            </g>
            <polygon points={hullOf(item)} class="hit" vector-effect="non-scaling-stroke" onpointerdown={(e) => onItemDown(e, item)} role="presentation" />
          </g>
        {/if}
      {/each}
    </g>

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
  </svg>

  {#if readout}
    <div class="readout tnum">{readout}</div>
  {/if}

  {#if !app.items.length && app.machines.length}
    <div class="empty">
      <div class="empty-card">
        <div class="empty-icon"><Icon name="layers" size={26} /></div>
        <h2>{t('empty.title')}</h2>
        <p>{t('empty.body')}</p>
        <div class="empty-actions">
          <button class="btn primary" onclick={() => app.open(() => backend.pickDesigns())}><Icon name="open" size={16} />{t('empty.open')}</button>
          <button class="btn" onclick={() => app.openDemo()}><Icon name="sparkle" size={16} />{t('empty.demo')}</button>
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
    width: 340px;
    padding: 26px 26px 22px;
    border-radius: 14px;
    background: var(--panel);
    background: color-mix(in srgb, var(--panel) 92%, transparent);
    backdrop-filter: blur(8px);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-2);
    text-align: center;
  }

  .empty-icon {
    display: inline-grid;
    place-items: center;
    width: 48px;
    height: 48px;
    margin-bottom: 10px;
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--accent);
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
