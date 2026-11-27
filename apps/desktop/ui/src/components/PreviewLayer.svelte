<script lang="ts">
  // The animated preview on the mat: what the virtual plotter cut, stroke by stroke, with the
  // head moving along. Lifted moves show as thin dashes.
  import { app } from '../lib/state.svelte';
  import { displayColor } from '../lib/theme.svelte';

  let { px }: { px: number } = $props();

  const timeline = $derived(app.timeline);
  const passes = $derived(app.preview?.sim?.plan.passes ?? []);
  const time = $derived(app.preview?.time ?? 0);

  function colorOf(pass: number | null): string {
    return displayColor(pass === null ? null : (passes[pass]?.color ?? null));
  }

  function pointsOf(points: number[]): string {
    let s = '';
    for (let k = 0; k < points.length; k += 2) s += `${points[k]},${points[k + 1]} `;
    return s;
  }

  const all = $derived(timeline ? timeline.strokes.map((s) => pointsOf(s.points)) : []);

  /** Index of the stroke under way at `time`. */
  const current = $derived.by(() => {
    if (!timeline) return 0;
    const strokes = timeline.strokes;
    let lo = 0;
    let hi = strokes.length - 1;
    if (time >= timeline.total) return strokes.length;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (strokes[mid].t0 <= time) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  });

  /** The part of the current stroke done so far, and where the head is. */
  const partial = $derived.by(() => {
    if (!timeline || current >= timeline.strokes.length) return null;
    const s = timeline.strokes[current];
    const f = s.dur > 0 ? Math.min(1, Math.max(0, (time - s.t0) / s.dur)) : 1;
    const len = s.cum[s.cum.length - 1];
    const target = f * len;
    let pts = '';
    let head = { x: s.points[0], y: s.points[1] };
    for (let k = 0; k < s.cum.length; k++) {
      const x = s.points[2 * k];
      const y = s.points[2 * k + 1];
      if (s.cum[k] <= target) {
        pts += `${x},${y} `;
        head = { x, y };
      } else {
        const prev = s.cum[k - 1] ?? 0;
        const t = (target - prev) / Math.max(1e-9, s.cum[k] - prev);
        const hx = s.points[2 * k - 2] + (x - s.points[2 * k - 2]) * t;
        const hy = s.points[2 * k - 1] + (y - s.points[2 * k - 1]) * t;
        pts += `${hx},${hy}`;
        head = { x: hx, y: hy };
        break;
      }
    }
    return { cut: s.cut, pass: s.pass, pts, head };
  });

  const end = $derived.by(() => {
    if (!timeline || !timeline.strokes.length) return { x: 0, y: 0 };
    const last = timeline.strokes[timeline.strokes.length - 1].points;
    return { x: last[last.length - 2], y: last[last.length - 1] };
  });
  const head = $derived(partial?.head ?? end);
</script>

{#if timeline}
  <g class="preview" pointer-events="none">
    {#each timeline.strokes as s, i (i)}
      {#if i < current}
        {#if s.cut}
          <polyline points={all[i]} class="cut" stroke={colorOf(s.pass)} vector-effect="non-scaling-stroke" />
        {:else}
          <polyline points={all[i]} class="travel" vector-effect="non-scaling-stroke" />
        {/if}
      {/if}
    {/each}
    {#if partial}
      <polyline points={partial.pts} class={partial.cut ? 'cut' : 'travel'} stroke={partial.cut ? colorOf(partial.pass) : undefined} vector-effect="non-scaling-stroke" />
    {/if}
    <g class="head" transform="translate({head.x} {head.y})">
      <circle r={9 * px} class="halo" />
      <circle r={3.2 * px} class="dot" class:down={partial?.cut} />
    </g>
  </g>
{/if}

<style>
  .cut {
    fill: none;
    stroke-width: 2.2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .travel {
    fill: none;
    stroke: var(--text-3);
    stroke-width: 1;
    stroke-dasharray: 3 3;
    opacity: 0.75;
  }

  .head .halo {
    fill: var(--accent);
    opacity: 0.16;
  }

  .head .dot {
    fill: var(--panel);
    stroke: var(--accent);
    stroke-width: 0.6;
  }

  .head .dot.down {
    fill: var(--accent);
  }
</style>
