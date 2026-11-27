<script lang="ts">
  // Position, size and angle of the selected design, as numbers.
  import { app } from '../lib/state.svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { designSize, normalizeAngle } from '../lib/geometry';
  import { fromUnit, toUnit, unitLabel } from '../lib/format';
  import NumberField from './NumberField.svelte';
  import Icon from './Icon.svelte';

  let keepRatio = $state(true);

  const item = $derived(app.selectedItem);
  const design = $derived(item ? app.designs[item.design] : null);
  const box = $derived(item ? app.boxes[item.id] : null);
  const size = $derived(item && design ? { w: Math.abs(item.sx) * designSize(design).w, h: Math.abs(item.sy) * designSize(design).h } : null);
  const digits = $derived(app.unit === 'in' ? 3 : 1);
  const step = $derived(app.unit === 'in' ? 0.0625 : 1);

  function setSize(axis: 'w' | 'h', value: number | null): void {
    if (!item || !design || !size || value === null || value <= 0) return;
    const mm = fromUnit(value);
    const own = designSize(design);
    app.checkpoint();
    if (axis === 'w') {
      const k = mm / size.w;
      item.sx = Math.sign(item.sx || 1) * (mm / own.w);
      if (keepRatio) item.sy *= k;
    } else {
      const k = mm / size.h;
      item.sy = Math.sign(item.sy || 1) * (mm / own.h);
      if (keepRatio) item.sx *= k;
    }
  }

  function setAngle(value: number | null): void {
    if (!item || value === null) return;
    app.checkpoint();
    item.angle = normalizeAngle(value);
  }
</script>

<section class="panel-section">
  <h3 class="panel-title">{t('item.title')}</h3>
  {#if item && design && box && size}
    <div class="name" title={design.name}><Icon name="layers" size={15} /><span>{design.name}</span></div>
    {#if app.outside.has(item.id)}
      <div class="outside"><Icon name="alert" size={14} />{t('item.outside')}</div>
    {/if}
    <div class="grid">
      <NumberField label={t('item.x')} value={toUnit(box.x0)} {digits} {step} unit={unitLabel()} onchange={(v) => app.moveSelectedTo(v === null ? null : fromUnit(v), null)} />
      <NumberField label={t('item.y')} value={toUnit(box.y0)} {digits} {step} unit={unitLabel()} onchange={(v) => app.moveSelectedTo(null, v === null ? null : fromUnit(v))} />
      <NumberField label={t('item.w')} value={toUnit(size.w)} {digits} {step} min={0.01} unit={unitLabel()} onchange={(v) => setSize('w', v)} />
      <NumberField label={t('item.h')} value={toUnit(size.h)} {digits} {step} min={0.01} unit={unitLabel()} onchange={(v) => setSize('h', v)} />
      <NumberField label={t('item.angle')} value={item.angle} digits={1} step={1} min={-360} max={360} unit="°" onchange={setAngle} />
      <div class="tools">
        <button class="icon-btn" class:active={keepRatio} title={t('item.keepRatio')} onclick={() => (keepRatio = !keepRatio)}><Icon name={keepRatio ? 'link' : 'unlink'} /></button>
        <button class="icon-btn" title={t('item.toOrigin')} onclick={() => app.moveSelectedTo(0, 0)}><Icon name="corner" /></button>
        <button class="icon-btn" title={t('item.center')} onclick={() => app.centerSelected()}><Icon name="center" /></button>
      </div>
    </div>
    {#if design.warnings.length}
      <div class="warnings">
        <div class="label">{t('item.warnings')}</div>
        {#each design.warnings as w (w)}<div class="hint">{w}</div>{/each}
      </div>
    {/if}
  {:else}
    <p class="hint">{t('item.none')}</p>
  {/if}
</section>

<style>
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    font-weight: 600;
    color: var(--text);
  }

  .name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name :global(.icon) {
    color: var(--text-3);
  }

  .outside {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: -2px 0 10px;
    padding: 6px 9px;
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 12px;
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 10px;
  }

  .tools {
    display: flex;
    align-items: flex-end;
    justify-content: flex-end;
    gap: 2px;
  }

  .warnings {
    margin-top: 12px;
  }
</style>
