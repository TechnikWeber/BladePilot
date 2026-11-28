<script lang="ts">
  // Position, size and angle of the selected design, how it is cut, and how a picture is traced.
  import { app } from '../lib/state.svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { designSize, normalizeAngle } from '../lib/geometry';
  import { fromUnit, toUnit, unitLabel } from '../lib/format';
  import NumberField from './NumberField.svelte';
  import Icon from './Icon.svelte';
  import type { TraceRequest } from '../lib/api/types';

  let keepRatio = $state(true);
  let copies = $state(4);
  let traceTimer: ReturnType<typeof setTimeout> | undefined;

  /** Traces the picture again a moment after the last change. */
  function retrace(patch: Partial<TraceRequest>): void {
    const image = design?.image;
    if (!design || !image) return;
    const trace = { ...image.trace, ...patch };
    app.designs[design.id] = { ...design, image: { ...image, trace } };
    clearTimeout(traceTimer);
    const id = design.id;
    traceTimer = setTimeout(() => void app.retrace(id, trace), 250);
  }

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
    <div class="name" title={design.name}>
      <Icon name={design.made ? (design.made.kind === 'text' ? 'text' : 'shapes') : 'layers'} size={15} /><span>{design.name}</span>
      {#if design.made}<button class="btn ghost small edit" onclick={() => app.editMade()}>{t('make.edit')}</button>{/if}
    </div>
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
        <button class="icon-btn" class:active={item.locked} title={item.locked ? t('item.unlock') : t('item.lock')} onclick={() => app.lockSelected(!item.locked)}><Icon name={item.locked ? 'lock' : 'unlock'} /></button>
        <button class="icon-btn" class:active={keepRatio} title={t('item.keepRatio')} onclick={() => (keepRatio = !keepRatio)}><Icon name={keepRatio ? 'link' : 'unlink'} /></button>
        <button class="icon-btn" title={t('item.toOrigin')} onclick={() => app.moveSelectedTo(0, 0)}><Icon name="corner" /></button>
        <button class="icon-btn" title={t('item.center')} onclick={() => app.centerSelected()}><Icon name="center" /></button>
      </div>
    </div>
    <div class="cut">
      <span class="label">{t('cut.title')}</span>
      <div class="segmented wide">
        <button class:on={item.cut.kind === 'lines'} onclick={() => app.setCut({ kind: 'lines', outerOnly: false, hidden: [] })}>{t('cut.lines')}</button>
        <button class:on={item.cut.kind === 'contour'} onclick={() => app.setCut({ kind: 'contour', offsetMm: 2 })}><Icon name="contour" size={14} />{t('cut.contour')}</button>
        <button class:on={item.cut.kind === 'none'} onclick={() => app.setCut({ kind: 'none' })}>{t('cut.none')}</button>
      </div>
      {#if item.cut.kind === 'lines'}
        {@const lines = item.cut}
        <label class="switch">
          <input type="checkbox" checked={lines.outerOnly} onchange={(e) => app.setCut({ ...lines, outerOnly: (e.target as HTMLInputElement).checked })} />
          <span class="track"></span>{t('cut.outerOnly')}
        </label>
        <label class="switch">
          <input type="checkbox" bind:checked={app.lineSelect} />
          <span class="track"></span>{t('cut.pick')}
        </label>
        {#if app.lineSelect}<p class="hint">{t('cut.pickHint')}</p>{/if}
        {#if !design.image}
          <button class="btn small tidy" title={t('tidy.hint')} onclick={() => app.tidySelected()}><Icon name="tidy" size={14} />{t('tidy.button')}</button>
        {/if}
        {#if lines.hidden.length}
          <div class="hidden-row">
            <span class="hint">{lines.hidden.length === 1 ? t('cut.hiddenOne') : t('cut.hidden', { n: lines.hidden.length })}</span>
            <button class="btn ghost small" onclick={() => app.setCut({ ...lines, hidden: [] })}>{t('cut.showAll')}</button>
          </div>
        {/if}
      {:else if item.cut.kind === 'contour'}
        <div class="offset">
          <NumberField
            label={t('cut.offset')}
            value={toUnit(item.cut.offsetMm)}
            digits={app.unit === 'in' ? 3 : 1}
            step={app.unit === 'in' ? 0.02 : 0.5}
            min={0}
            unit={unitLabel()}
            onchange={(v) => v !== null && app.setCut({ kind: 'contour', offsetMm: Math.max(0, Math.min(30, fromUnit(v))) })}
          />
        </div>
        <p class="hint">{t('cut.contourHint')}</p>
      {/if}
      {#if app.printMode}
        <label class="switch" title={design.printable ? '' : t('print.notPrintable')}>
          <input type="checkbox" checked={item.print && !!design.printable} disabled={!design.printable} onchange={(e) => (item.print = (e.target as HTMLInputElement).checked)} />
          <span class="track"></span>{t('print.item')}
        </label>
      {/if}
    </div>
    {#if item.cut.kind !== 'none'}
      {@const weed = item.weed}
      <div class="cut">
        <span class="label">{t('weed.title')}</span>
        <label class="switch">
          <input type="checkbox" checked={!!weed} onchange={(e) => app.setWeed((e.target as HTMLInputElement).checked ? { marginMm: 3, lines: true } : null)} />
          <span class="track"></span>{t('weed.box')}
        </label>
        {#if weed}
          <div class="offset">
            <NumberField
              label={t('weed.margin')}
              value={toUnit(weed.marginMm)}
              digits={app.unit === 'in' ? 3 : 1}
              step={app.unit === 'in' ? 0.02 : 0.5}
              min={0}
              unit={unitLabel()}
              onchange={(v) => v !== null && app.setWeed({ ...weed, marginMm: Math.max(0.5, Math.min(50, fromUnit(v))) })}
            />
          </div>
          <label class="switch">
            <input type="checkbox" checked={weed.lines} onchange={(e) => app.setWeed({ ...weed, lines: (e.target as HTMLInputElement).checked })} />
            <span class="track"></span>{t('weed.lines')}
          </label>
          <p class="hint">{t('weed.hint')}</p>
        {/if}
      </div>
    {/if}
    <div class="cut copies">
      <NumberField label={t('copies.label')} value={copies} digits={0} step={1} min={1} max={200} onchange={(v) => (copies = Math.max(1, Math.min(200, Math.round(v ?? 1))))} />
      <button class="btn" onclick={() => app.addCopies(copies)}><Icon name="copies" size={15} />{t('copies.add')}</button>
    </div>
    {#if design.image}
      {@const trace = design.image.trace}
      <div class="cut">
        <span class="label">{t('trace.title')}</span>
        <div class="trace-row">
          <span class="hint">{t('trace.source')}</span>
          <div class="segmented">
            <button class:on={trace.source === 'alpha'} onclick={() => retrace({ source: 'alpha' })}>{t('trace.alpha')}</button>
            <button class:on={trace.source === 'darkness'} onclick={() => retrace({ source: 'darkness' })}>{t('trace.darkness')}</button>
          </div>
        </div>
        <label class="range">
          <span class="hint">{t('trace.threshold')}</span>
          <input type="range" min="0.05" max="0.95" step="0.01" value={trace.threshold} oninput={(e) => retrace({ threshold: Number((e.target as HTMLInputElement).value) })} />
          <span class="tnum value">{Math.round(trace.threshold * 100)} %</span>
        </label>
        <div class="trace-row">
          <NumberField
            label={t('trace.despeckle')}
            value={trace.despeckleMm2}
            digits={1}
            step={0.5}
            min={0}
            unit="mm²"
            onchange={(v) => v !== null && retrace({ despeckleMm2: Math.max(0, Math.min(500, v)) })}
          />
          <label class="check">
            <input type="checkbox" checked={trace.invert} onchange={(e) => retrace({ invert: (e.target as HTMLInputElement).checked })} />
            {t('trace.invert')}
          </label>
        </div>
      </div>
    {/if}
    {#if design.warnings.length}
      <div class="warnings">
        <div class="label">{t('item.warnings')}</div>
        {#each design.warnings as w (w)}<div class="hint">{w}</div>{/each}
      </div>
    {/if}
  {:else if app.selectedItems.length > 1}
    {@const items = app.selectedItems}
    {@const grouped = items.some((i) => i.group !== null)}
    {@const locked = items.some((i) => i.locked)}
    <div class="name"><Icon name="layers" size={15} /><span>{t('multi.count', { n: items.length })}</span></div>
    <span class="label">{t('multi.align')}</span>
    <div class="icon-row">
      <button class="icon-btn" title={t('multi.alignLeft')} disabled={locked} onclick={() => app.alignSelected('left')}><Icon name="alignLeft" /></button>
      <button class="icon-btn" title={t('multi.alignHCenter')} disabled={locked} onclick={() => app.alignSelected('hcenter')}><Icon name="alignHCenter" /></button>
      <button class="icon-btn" title={t('multi.alignRight')} disabled={locked} onclick={() => app.alignSelected('right')}><Icon name="alignRight" /></button>
      <span class="gap"></span>
      <button class="icon-btn" title={t('multi.alignTop')} disabled={locked} onclick={() => app.alignSelected('top')}><Icon name="alignTop" /></button>
      <button class="icon-btn" title={t('multi.alignVCenter')} disabled={locked} onclick={() => app.alignSelected('vcenter')}><Icon name="alignVCenter" /></button>
      <button class="icon-btn" title={t('multi.alignBottom')} disabled={locked} onclick={() => app.alignSelected('bottom')}><Icon name="alignBottom" /></button>
      <span class="gap"></span>
      <button class="icon-btn" title={t('multi.distributeX')} disabled={locked || items.length < 3} onclick={() => app.distributeSelected('x')}><Icon name="distributeX" /></button>
      <button class="icon-btn" title={t('multi.distributeY')} disabled={locked || items.length < 3} onclick={() => app.distributeSelected('y')}><Icon name="distributeY" /></button>
    </div>
    <div class="multi-actions">
      {#if grouped}
        <button class="btn" onclick={() => app.ungroupSelected()}><Icon name="ungroup" size={15} />{t('multi.ungroup')}</button>
      {:else}
        <button class="btn" onclick={() => app.groupSelected()}><Icon name="group" size={15} />{t('multi.group')}</button>
      {/if}
      <button class="btn" onclick={() => app.lockSelected(!locked)}><Icon name={locked ? 'unlock' : 'lock'} size={15} />{locked ? t('item.unlock') : t('item.lock')}</button>
      <button class="btn" title={t('weld.hint')} onclick={() => app.weldSelected()}><Icon name="weld" size={15} />{t('weld.button')}</button>
      <button class="btn danger-ghost" onclick={() => app.removeSelected()}><Icon name="trash" size={15} />{t('top.delete')}</button>
    </div>
    <p class="hint">{t('multi.hint')}</p>
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

  .name .edit {
    margin-left: auto;
  }

  .icon-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    margin: 4px 0 12px;
  }

  .icon-row .gap {
    width: 8px;
  }

  .multi-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .multi-actions .btn {
    justify-content: center;
  }

  .danger-ghost {
    color: var(--danger);
  }

  .tidy {
    align-self: flex-start;
  }

  .copies {
    flex-direction: row;
    align-items: flex-end;
  }

  .copies :global(.nf) {
    flex: 0 0 90px;
  }

  .cut {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .cut .label {
    margin: 0;
  }

  .segmented.wide {
    display: flex;
  }

  .segmented.wide button {
    flex: 1;
    justify-content: center;
  }

  .hidden-row,
  .trace-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .trace-row :global(.nf) {
    flex: 1;
  }

  .btn.small {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }

  .offset {
    max-width: 50%;
  }

  .range {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
  }

  .range input {
    accent-color: var(--accent);
  }

  .range .value {
    min-width: 38px;
    text-align: right;
    font-size: 12px;
    color: var(--text-2);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 18px;
    font-size: 12.5px;
  }

  .check input {
    accent-color: var(--accent);
  }
</style>
