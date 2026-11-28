<script lang="ts">
  // Make a text or a shape, or change one on the mat: settings on the left, a live preview made
  // by the Rust side (the real letter outlines) on the right.
  import { untrack } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { t, errorText } from '../lib/i18n/index.svelte';
  import { backend } from '../lib/api';
  import { fromUnit, toUnit, unitLabel } from '../lib/format';
  import type { ApiError, DesignInfo, MadeSpec, ShapeName } from '../lib/api/types';
  import NumberField from './NumberField.svelte';
  import Icon from './Icon.svelte';

  const SHAPES: ShapeName[] = ['rectangle', 'ellipse', 'polygon', 'star', 'heart'];

  let spec = $state<MadeSpec | null>(null);
  let preview = $state<DesignInfo | null>(null);
  let previewKey = $state('');
  let error = $state<ApiError | null>(null);
  let busy = $state(false);
  /** Designs made for the preview and not used: forgotten when the dialog closes. */
  let made: number[] = [];

  // A fresh copy of the spec whenever the dialog opens.
  $effect(() => {
    const make = app.make;
    untrack(() => {
      spec = make ? structuredClone($state.snapshot(make.spec)) : null;
      preview = null;
      previewKey = '';
      error = null;
    });
  });

  const digits = $derived(app.unit === 'in' ? 2 : 1);
  const step = $derived(app.unit === 'in' ? 0.125 : 1);

  // The preview, a moment after the last change.
  $effect(() => {
    if (!spec) return;
    const key = JSON.stringify(spec);
    if (spec.kind === 'text' && !spec.text.trim()) {
      preview = null;
      previewKey = key;
      error = null;
      return;
    }
    const timer = setTimeout(() => void render(key), 220);
    return () => clearTimeout(timer);
  });

  async function render(key: string): Promise<DesignInfo | null> {
    if (key === previewKey && preview) return preview;
    try {
      const design = await backend.makeDesign(JSON.parse(key) as MadeSpec);
      made.push(design.id);
      if (spec && JSON.stringify(spec) === key) {
        preview = design;
        previewKey = key;
        error = null;
      }
      return design;
    } catch (e) {
      error = e as ApiError;
      return null;
    }
  }

  function close(): void {
    for (const id of made) void backend.forgetDesign(id);
    made = [];
    app.make = null;
  }

  async function apply(): Promise<void> {
    if (!spec) return;
    busy = true;
    const design = await render(JSON.stringify(spec));
    busy = false;
    if (!design || !app.make) return;
    for (const id of made) if (id !== design.id) void backend.forgetDesign(id);
    made = [];
    app.applyMade(design);
  }

  const editing = $derived(app.make?.itemId ?? null);
  const viewBox = $derived.by(() => {
    if (!preview) return '0 0 10 10';
    const [x0, y0, x1, y1] = preview.bbox;
    const pad = Math.max(x1 - x0, y1 - y0) * 0.06 + 0.5;
    return `${x0 - pad} ${y0 - pad} ${x1 - x0 + 2 * pad} ${y1 - y0 + 2 * pad}`;
  });
  const sizeText = $derived.by(() => {
    if (!preview) return '';
    const [x0, y0, x1, y1] = preview.bbox;
    return `${toUnit(x1 - x0).toFixed(digits)} × ${toUnit(y1 - y0).toFixed(digits)} ${unitLabel()}`;
  });

  function onKey(e: KeyboardEvent): void {
    if (e.key === 'Escape') close();
  }
</script>

{#if app.make && spec}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" role="presentation" onkeydown={onKey}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="make-title">
      <header>
        <div class="title-icon"><Icon name={spec.kind === 'text' ? 'text' : 'shapes'} size={18} /></div>
        <h2 id="make-title">{spec.kind === 'text' ? t('make.titleText') : t('make.titleShape')}</h2>
        <button class="icon-btn" aria-label={t('make.cancel')} onclick={close}><Icon name="close" /></button>
      </header>
      <div class="body">
        <div class="form">
          {#if spec.kind === 'text'}
            {@const text = spec}
            <label class="field">
              <span class="label">{t('make.text')}</span>
              <!-- svelte-ignore a11y_autofocus -->
              <textarea rows="3" bind:value={text.text} placeholder={t('make.textPlaceholder')} autofocus></textarea>
            </label>
            <label class="field">
              <span class="label">{t('make.font')}</span>
              <select bind:value={text.family} disabled={app.fonts === null}>
                <option value="">{app.fonts === null ? t('make.fontsLoading') : t('make.fontDefault')}</option>
                {#each app.fonts ?? [] as family (family)}
                  <option value={family} style="font-family: '{family}'">{family}</option>
                {/each}
              </select>
            </label>
            <div class="row">
              <NumberField label={t('make.size')} value={toUnit(text.sizeMm)} {digits} {step} min={toUnit(1)} unit={unitLabel()} onchange={(v) => v !== null && (text.sizeMm = fromUnit(v))} />
              <NumberField label={t('make.spacing')} value={toUnit(text.letterSpacingMm)} digits={app.unit === 'in' ? 3 : 1} step={app.unit === 'in' ? 0.02 : 0.5} unit={unitLabel()} onchange={(v) => (text.letterSpacingMm = v === null ? 0 : fromUnit(v))} />
            </div>
            <div class="row">
              <div class="segmented">
                <button class:on={text.bold} onclick={() => (text.bold = !text.bold)} title={t('make.bold')}><b>B</b></button>
                <button class:on={text.italic} onclick={() => (text.italic = !text.italic)} title={t('make.italic')}><i>I</i></button>
              </div>
              <div class="segmented">
                <button class:on={text.align === 'left'} onclick={() => (text.align = 'left')}>{t('make.alignLeft')}</button>
                <button class:on={text.align === 'center'} onclick={() => (text.align = 'center')}>{t('make.alignCenter')}</button>
                <button class:on={text.align === 'right'} onclick={() => (text.align = 'right')}>{t('make.alignRight')}</button>
              </div>
            </div>
            {#if text.text.includes('\n')}
              <div class="row">
                <NumberField label={t('make.lineHeight')} value={text.lineHeight} digits={2} step={0.05} min={0.5} max={4} unit="×" onchange={(v) => v !== null && (text.lineHeight = v)} />
              </div>
            {/if}
            <p class="hint">{t('make.textHint')}</p>
          {:else}
            {@const shape = spec}
            <div class="shapes">
              {#each SHAPES as name (name)}
                <button class="shape-btn" class:on={shape.shape === name} title={t(`shape.${name}`)} onclick={() => (shape.shape = name)}>
                  <Icon name={name} size={20} /><span>{t(`shape.${name}`)}</span>
                </button>
              {/each}
            </div>
            <div class="row">
              <NumberField label={t('make.width')} value={toUnit(shape.widthMm)} {digits} {step} min={toUnit(0.5)} unit={unitLabel()} onchange={(v) => v !== null && (shape.widthMm = fromUnit(v))} />
              <NumberField label={t('make.height')} value={toUnit(shape.heightMm)} {digits} {step} min={toUnit(0.5)} unit={unitLabel()} onchange={(v) => v !== null && (shape.heightMm = fromUnit(v))} />
            </div>
            {#if shape.shape === 'rectangle'}
              <div class="row">
                <NumberField label={t('make.corner')} value={toUnit(shape.cornerMm)} {digits} {step} min={0} unit={unitLabel()} onchange={(v) => (shape.cornerMm = v === null ? 0 : fromUnit(v))} />
              </div>
            {:else if shape.shape === 'polygon' || shape.shape === 'star'}
              <div class="row">
                <NumberField label={shape.shape === 'star' ? t('make.points') : t('make.sides')} value={shape.sides} digits={0} step={1} min={3} max={64} onchange={(v) => v !== null && (shape.sides = Math.round(v))} />
                {#if shape.shape === 'star'}
                  <NumberField label={t('make.inner')} value={Math.round(shape.inner * 100)} digits={0} step={5} min={10} max={95} unit="%" onchange={(v) => v !== null && (shape.inner = v / 100)} />
                {/if}
              </div>
            {/if}
          {/if}
        </div>
        <div class="preview">
          {#if preview}
            <svg viewBox={viewBox} preserveAspectRatio="xMidYMid meet">
              {#each preview.shapes as shape, i (i)}
                <path d={shape.d} fill={shape.fill ? 'var(--text)' : 'none'} fill-opacity="0.85" stroke="var(--text)" vector-effect="non-scaling-stroke" />
              {/each}
            </svg>
            <span class="size tnum">{sizeText}</span>
          {:else if error}
            <p class="error"><Icon name="alert" size={15} />{errorText(error)}</p>
          {:else}
            <p class="hint">{spec.kind === 'text' ? t('make.empty') : ''}</p>
          {/if}
        </div>
      </div>
      <footer>
        <span class="spacer"></span>
        <button class="btn ghost" onclick={close}>{t('make.cancel')}</button>
        <button class="btn primary" disabled={busy || (spec.kind === 'text' && !spec.text.trim())} onclick={apply}>
          <Icon name="check" size={14} />{editing !== null ? t('make.apply') : t('make.add')}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: rgba(10, 16, 24, 0.42);
  }

  .dialog {
    width: min(760px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    overflow: auto;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-3);
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 16px 4px 20px;
  }

  header h2 {
    flex: 1;
    margin: 0;
    font-size: 16px;
  }

  .title-icon {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 18px;
    padding: 12px 20px 4px;
  }

  @media (max-width: 640px) {
    .body {
      grid-template-columns: 1fr;
    }
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  textarea,
  select {
    width: 100%;
    font: inherit;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
  }

  textarea {
    resize: vertical;
    min-height: 64px;
    font-size: 15px;
  }

  textarea:focus,
  select:focus {
    outline: 2px solid var(--accent-soft);
    border-color: var(--accent);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: flex-end;
  }

  .row > :global(*) {
    flex: 1 1 120px;
  }

  .row > .segmented {
    flex: 0 0 auto;
  }

  .shapes {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 6px;
  }

  .shape-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 2px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel-2);
    color: var(--text-2);
    font-size: 11px;
    min-width: 0;
  }

  .shape-btn span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .shape-btn.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }

  .preview {
    position: relative;
    display: grid;
    place-items: center;
    min-height: 220px;
    border-radius: var(--radius);
    background: var(--panel-2);
    border: 1px solid var(--border);
    padding: 14px;
  }

  .preview svg {
    width: 100%;
    height: 100%;
    max-height: 300px;
  }

  .preview path {
    stroke-width: 1.4;
    stroke-linejoin: round;
  }

  .size {
    position: absolute;
    right: 10px;
    bottom: 8px;
    font-size: 11.5px;
    color: var(--text-2);
  }

  .error {
    display: flex;
    gap: 8px;
    color: var(--danger);
    font-size: 12.5px;
  }

  .hint {
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 20px 18px;
  }

  .spacer {
    flex: 1;
  }
</style>
