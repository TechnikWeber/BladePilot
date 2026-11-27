<script lang="ts">
  // Controls of the animated preview: play, pause, scrub, speed, and what is being cut.
  import { app } from '../lib/state.svelte';
  import { t, num, errorText } from '../lib/i18n/index.svelte';
  import { displayColor } from '../lib/theme.svelte';
  import { fmtLength } from '../lib/format';
  import Icon from './Icon.svelte';

  const p = $derived(app.preview);
  const timeline = $derived(app.timeline);
  const sim = $derived(p?.sim ?? null);
  const speeds = [1, 2, 4, 8, 16, 32, 64];

  // The animation clock.
  $effect(() => {
    if (!p?.playing || !timeline) return;
    let last = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      const preview = app.preview;
      if (!preview || !app.timeline) return;
      preview.time = Math.min(app.timeline.total, preview.time + (now - last) * preview.speed);
      last = now;
      if (preview.time >= app.timeline.total) preview.playing = false;
      else frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  });

  const pass = $derived.by(() => {
    if (!timeline || !p) return -1;
    let index = -1;
    timeline.passStarts.forEach((start, i) => {
      if (start !== undefined && start <= p.time) index = i;
    });
    return index;
  });
  const passInfo = $derived(sim && pass >= 0 ? sim.plan.passes[pass] : null);
  const done = $derived(!!timeline && !!p && p.time >= timeline.total);

  function toggle(): void {
    if (!p || !timeline) return;
    if (done) p.time = 0;
    p.playing = !p.playing;
  }

  function seek(e: Event): void {
    if (!p || !timeline) return;
    p.time = (Number((e.target as HTMLInputElement).value) / 1000) * timeline.total;
  }

</script>

{#if p}
  <div class="bar">
    {#if p.loading}
      <div class="loading"><span class="spinner"></span>{t('status.planning')}</div>
    {:else if p.error}
      <div class="error"><Icon name="alert" size={16} />{t('preview.failed')}: {errorText(p.error)}</div>
      <button class="icon-btn" title={t('preview.close')} onclick={() => app.closePreview()}><Icon name="close" /></button>
    {:else if sim && timeline}
      <button class="play" title={p.playing ? t('preview.pause') : t('preview.play')} onclick={toggle}>
        <Icon name={p.playing ? 'pause' : done ? 'restart' : 'play'} size={18} />
      </button>
      <div class="middle">
        <div class="info">
          {#if done}
            <span class="pass"><Icon name="check" size={14} />{t('preview.done')}</span>
          {:else if passInfo}
            <span class="pass">
              <span class="swatch" style="background: {displayColor(passInfo.color)}"></span>
              {t('preview.pass', { n: pass + 1, total: sim.plan.passes.length })}
              <span class="tool">· {passInfo.settings.tool === 'pen' ? t('tool.pen') : t('tool.blade')}</span>
            </span>
          {:else}
            <span class="pass">{t('preview.travel')}</span>
          {/if}
          <span class="spacer"></span>
          <span class="tnum dim">{fmtLength(sim.cutMm)} · {num(timeline.total ? (p.time / timeline.total) * 100 : 0, 0)} %</span>
        </div>
        <input class="scrub" type="range" min="0" max="1000" step="1" value={timeline.total ? (p.time / timeline.total) * 1000 : 0} oninput={seek} />
      </div>
      <div class="speed segmented" title={t('preview.speed')}>
        {#each speeds.filter((s) => s >= 1 && s <= Math.max(8, p.speed * 2)).slice(0, 5) as s (s)}
          <button class:on={p.speed === s} onclick={() => p && (p.speed = s)}>{num(s, 0)}×</button>
        {/each}
      </div>
      <button class="icon-btn" title={t('preview.close')} onclick={() => app.closePreview()}><Icon name="close" /></button>
    {/if}
  </div>
  {#if sim?.warnings.length}
    <div class="warnings">
      <Icon name="alert" size={15} />
      <div>
        <strong>{t('preview.warnings')}</strong>
        {#each sim.warnings.slice(0, 4) as w (w)}<div>{w}</div>{/each}
      </div>
    </div>
  {/if}
{/if}

<style>
  .bar {
    position: absolute;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    width: min(680px, calc(100% - 32px));
    padding: 10px 12px 10px 10px;
    border-radius: 14px;
    background: var(--panel);
    background: color-mix(in srgb, var(--panel) 94%, transparent);
    backdrop-filter: blur(10px);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-2);
  }

  .play {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: var(--shadow-1);
  }

  .play:hover {
    background: var(--accent-hover);
  }

  .middle {
    flex: 1;
    min-width: 0;
  }

  .info {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 12.5px;
  }

  .pass {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-weight: 600;
    white-space: nowrap;
  }

  .tool,
  .dim {
    color: var(--text-3);
    font-weight: 500;
  }

  .spacer {
    flex: 1;
  }

  .scrub {
    width: 100%;
    height: 18px;
    margin: 0;
    accent-color: var(--accent);
  }

  .speed button {
    padding: 0 8px;
  }

  .loading,
  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    padding: 6px 4px;
    color: var(--text-2);
  }

  .error {
    color: var(--danger);
  }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .warnings {
    position: absolute;
    top: 14px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 10px;
    max-width: 560px;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--panel);
    border: 1px solid var(--warn);
    color: var(--warn);
    box-shadow: var(--shadow-2);
    font-size: 12.5px;
  }

  .warnings div div {
    color: var(--text-2);
  }
</style>
