<script lang="ts">
  // Material, tool, force, speed, depth and repeats of one pass.
  import { TOOLS, type Settings, type Tool } from '../lib/api/types';
  import { t } from '../lib/i18n/index.svelte';
  import { app, toolName } from '../lib/state.svelte';
  import { matchMaterial, materialsFor } from '../lib/materials';
  import NumberField from './NumberField.svelte';
  import SliderField from './SliderField.svelte';
  import Icon from './Icon.svelte';

  let { settings, onchange }: { settings: Settings; onchange: () => void } = $props();
  let more = $state(false);
  let naming = $state(false);
  let name = $state('');

  const choices = $derived(materialsFor(settings.tool, app.materials));
  const material = $derived(matchMaterial(settings, choices));
  const pen = $derived(settings.tool === 'pen');

  function setTool(tool: Tool): void {
    const wasPen = settings.tool === 'pen';
    settings.tool = tool;
    // Between pen and blade the numbers mean something else: start from that kind's first material.
    if (wasPen !== (tool === 'pen')) {
      const first = materialsFor(tool, app.materials)[0];
      if (first) app.applyMaterial(settings, first);
      settings.tool = tool;
    }
    onchange();
  }

  function choose(id: string): void {
    const m = app.materials.find((x) => x.id === id);
    if (!m) return;
    app.applyMaterial(settings, m);
    onchange();
  }

  function saveAs(): void {
    app.saveMaterial(name, settings);
    naming = false;
    name = '';
  }

  function set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    settings[key] = value;
    onchange();
  }
</script>

<div class="editor">
  <div class="material-row">
    <select class="material" value={material?.id ?? ''} onchange={(e) => choose((e.target as HTMLSelectElement).value)} aria-label={t('material.title')}>
      <option value="" disabled>{t('material.own')}</option>
      {#each choices as m (m.id)}
        <option value={m.id}>{app.materialName(m)}</option>
      {/each}
    </select>
    {#if material && !material.builtin}
      <button class="icon-btn" title={t('material.delete')} onclick={() => app.deleteMaterial(material.id)}><Icon name="trash" size={15} /></button>
    {:else if !material}
      <button class="icon-btn" title={t('material.saveAs')} onclick={() => (naming = !naming)}><Icon name="check" size={15} /></button>
    {/if}
  </div>
  {#if naming && !material}
    <form class="name-row" onsubmit={(e) => { e.preventDefault(); saveAs(); }}>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" bind:value={name} placeholder={t('material.namePlaceholder')} autofocus />
      <button class="btn small" type="submit" disabled={!name.trim()}>{t('material.save')}</button>
    </form>
  {/if}
  {#if material?.mirror}<p class="hint material-hint mirror-hint"><Icon name="mirrorH" size={13} />{t('material.mirrorHint')}</p>{/if}
  {#if material?.builtin}<p class="hint material-hint">{t('material.hint')}</p>{/if}
  <div class="tool-row">
    <span class="label"><Icon name={pen ? 'pen' : 'blade'} size={14} />{t('settings.tool')}</span>
    <select class="tool" value={settings.tool} onchange={(e) => setTool((e.target as HTMLSelectElement).value as Tool)} aria-label={t('settings.tool')}>
      {#each TOOLS as tool (tool)}
        <option value={tool}>{toolName(tool)}</option>
      {/each}
    </select>
  </div>
  <div class="sliders">
    <SliderField
      label={t('settings.force')}
      title={t('settings.machineHint')}
      value={settings.force}
      optional
      min={1}
      max={33}
      low={t('settings.forceLow')}
      high={t('settings.forceHigh')}
      emptyText={t('settings.machine')}
      onchange={(v) => set('force', v)}
    />
    <SliderField
      label={t('settings.speed')}
      title={t('settings.machineHint')}
      value={settings.speed}
      optional
      min={1}
      max={30}
      low={t('settings.speedLow')}
      high={t('settings.speedHigh')}
      emptyText={t('settings.machine')}
      onchange={(v) => set('speed', v)}
    />
    {#if !pen}
      <SliderField
        label={settings.tool === 'autoBlade' ? t('settings.depth') : t('settings.depthByHand')}
        title={settings.tool === 'autoBlade' ? t('settings.machineHint') : t('settings.depthByHandHint')}
        value={settings.depth}
        optional
        min={0}
        max={10}
        low={t('settings.depthLow')}
        high={t('settings.depthHigh')}
        emptyText={settings.tool === 'autoBlade' ? t('settings.machine') : '–'}
        onchange={(v) => set('depth', v)}
      />
    {/if}
  </div>
  <div class="grid">
    <NumberField label={t('settings.repeats')} value={settings.repeats} min={1} max={10} step={1} digits={0} unit="×" onchange={(v) => set('repeats', v ?? 1)} />
  </div>
  {#if !pen}
    <button class="more" onclick={() => (more = !more)}>
      <Icon name={more ? 'chevronDown' : 'chevronRight'} size={13} />{more ? t('settings.less') : t('settings.more')}
    </button>
    {#if more}
      <div class="grid">
        <NumberField label={t('settings.overcut')} value={settings.overcutMm} min={0} max={2} step={0.1} digits={2} unit="mm" onchange={(v) => set('overcutMm', v ?? 0)} />
        <NumberField label={t('settings.bladeOffset')} value={settings.bladeOffsetMm} min={0} max={2} step={0.05} digits={2} unit="mm" onchange={(v) => set('bladeOffsetMm', v ?? 0.9)} />
      </div>
    {/if}
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .material-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .material {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 8px;
    font: inherit;
    font-size: 12.5px;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .name-row {
    display: flex;
    gap: 6px;
  }

  .name-row input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 8px;
    font: inherit;
    font-size: 12.5px;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .material-hint {
    margin: -4px 0 0;
    font-size: 11.5px;
  }

  .tool-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .tool-row .label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }

  .tool {
    height: 30px;
    padding: 0 8px;
    font: inherit;
    font-size: 12.5px;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 10px;
  }

  .sliders {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .mirror-hint {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--accent);
  }

  .mirror-hint :global(svg) {
    flex: none;
    margin-top: 1px;
  }

  .more {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    font-weight: 550;
  }

  .more:hover {
    color: var(--accent);
  }
</style>
