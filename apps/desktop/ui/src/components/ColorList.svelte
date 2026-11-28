<script lang="ts">
  // The passes: one per colour (or one for everything), in cutting order, each with its tool and
  // settings. Drag a pass by its grip to choose the order yourself.
  import { app, colorKey, type ColorRow } from '../lib/state.svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { displayColor } from '../lib/theme.svelte';
  import { fmtLength } from '../lib/format';
  import { colorName } from '../lib/colors';
  import type { Settings } from '../lib/api/types';
  import { matchMaterial } from '../lib/materials';
  import SettingsEditor from './SettingsEditor.svelte';
  import Icon from './Icon.svelte';

  let open = $state<string | null>(null);
  let dragKey = $state<string | null>(null);
  let dragOrder = $state<(string | null)[] | null>(null);
  let listEl: HTMLDivElement | undefined = $state();

  const rows = $derived.by(() => {
    if (!dragOrder) return app.rows;
    const byKey = new Map(app.rows.map((r) => [r.key, r]));
    return dragOrder.map((c) => byKey.get(colorKey(c))).filter((r): r is ColorRow => !!r);
  });

  function summary(s: Settings): string {
    const material = matchMaterial(s, app.materials);
    if (material) return app.materialName(material);
    const parts = [s.tool === 'pen' ? t('settings.pen') : t('settings.blade')];
    if (s.force !== null) parts.push(`${t('settings.force')} ${s.force}`);
    if (s.speed !== null) parts.push(`${t('settings.speed')} ${s.speed}`);
    if (s.tool === 'blade' && s.depth !== null) parts.push(`${t('settings.depth')} ${s.depth}`);
    if (s.repeats > 1) parts.push(`${s.repeats}×`);
    return parts.join(' · ');
  }

  function save(): void {
    app.savePrefs();
  }

  function toggleOpen(row: ColorRow): void {
    if (open === row.key) {
      open = null;
      return;
    }
    app.editPrefs(row.color);
    open = row.key;
  }

  function toggleSkip(row: ColorRow): void {
    const p = app.editPrefs(row.color);
    p.skip = !p.skip;
    save();
  }

  function conflictWith(row: ColorRow): number | null {
    const plan = app.plan;
    if (!plan || row.passIndex < 0) return null;
    const c = plan.conflicts.find(([a]) => a === row.passIndex);
    return c ? c[1] + 1 : null;
  }

  // Reordering with the pointer (HTML drag and drop is not available in every web view).
  function gripDown(e: PointerEvent, row: ColorRow): void {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragKey = row.key;
    dragOrder = app.rows.map((r) => r.color);
  }

  function gripMove(e: PointerEvent): void {
    if (!dragKey || !dragOrder || !listEl) return;
    const items = Array.from(listEl.querySelectorAll<HTMLElement>('[data-row]'));
    const from = dragOrder.findIndex((c) => colorKey(c) === dragKey);
    let to = items.length - 1;
    for (let i = 0; i < items.length; i++) {
      const r = items[i].getBoundingClientRect();
      if (e.clientY < r.top + r.height / 2) {
        to = i;
        break;
      }
    }
    if (to !== from && from >= 0) {
      const next = [...dragOrder];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      dragOrder = next;
    }
  }

  function gripUp(): void {
    if (dragOrder && dragKey) {
      const changed = dragOrder.some((c, i) => app.rows[i]?.color !== c);
      if (changed) {
        app.order = dragOrder;
        app.manualOrder = true;
      }
    }
    dragKey = null;
    dragOrder = null;
  }
</script>

<section class="panel-section">
  <h3 class="panel-title">{t('colors.title')}<span class="spacer"></span>
    {#if app.plan}<span class="count tnum">{app.plan.passes.length}</span>{/if}
  </h3>

  <label class="switch">
    <input type="checkbox" checked={app.byColor} onchange={(e) => { app.byColor = (e.target as HTMLInputElement).checked; save(); }} />
    <span class="track"></span>{t('colors.byColor')}
  </label>
  {#if app.byColor}
    <label class="switch auto" title={app.manualOrder ? t('colors.manualHint') : t('colors.autoHint')}>
      <input type="checkbox" checked={!app.manualOrder} onchange={(e) => (app.manualOrder = !(e.target as HTMLInputElement).checked)} />
      <span class="track"></span>{t('colors.auto')}
    </label>
    <p class="hint explain">{app.manualOrder ? t('colors.manualHint') : t('colors.autoHint')}</p>
  {/if}
  <label class="switch mirror" title={t('colors.mirrorHint')}>
    <input type="checkbox" checked={app.mirrored} onchange={(e) => app.setMirror((e.target as HTMLInputElement).checked)} />
    <span class="track"></span>{t('colors.mirror')}
  </label>
  {#if app.mirrored}<p class="hint explain">{t('colors.mirrorHint')}</p>{/if}

  {#if !rows.length}
    <p class="hint">{t('colors.empty')}</p>
  {:else}
    <div class="list" bind:this={listEl}>
      {#each rows as row, i (row.key)}
        {@const skip = row.prefs.skip}
        {@const conflict = conflictWith(row)}
        {@const stored = app.prefs[row.key]}
        <div class="row" class:skip class:open={open === row.key && app.byColor} class:dragging={dragKey === row.key} data-row>
          <div class="head">
            {#if app.byColor}
              <span
                class="grip"
                title={t('colors.drag')}
                onpointerdown={(e) => gripDown(e, row)}
                onpointermove={gripMove}
                onpointerup={gripUp}
                onpointercancel={gripUp}
                role="button"
                tabindex="-1"
              ><Icon name="grip" size={14} /></span>
              <span class="num tnum">{skip ? '–' : row.passIndex >= 0 ? row.passIndex + 1 : i + 1}</span>
            {/if}
            <span class="swatch" class:none={!row.color} style={row.color ? `background: ${displayColor(row.color)}` : ''}></span>
            <button class="main" onclick={() => toggleOpen(row)} disabled={!app.byColor}>
              <span class="name">{colorName(row.color)}{#if row.color}<span class="hex">{row.color.toUpperCase()}</span>{/if}</span>
              <span class="sub">
                {#if skip}{t('colors.skipped')}{:else if app.byColor}{summary(row.prefs.settings)}{:else}{row.info.shapes === 1 ? t('colors.shape') : t('colors.shapes', { n: row.info.shapes })} · {fmtLength(row.info.lengthMm)}{/if}
              </span>
            </button>
            {#if row.pass?.toolChange && !skip}
              <span class="flag tool" title={t(row.pass.settings.tool === 'pen' ? 'colors.toolChange.pen' : 'colors.toolChange.blade')}><Icon name="hand" size={14} /></span>
            {:else if row.pass?.pauseBefore && !skip}
              <span class="flag tool" title={t('colors.pause')}><Icon name="pause" size={12} /></span>
            {/if}
            {#if conflict && !skip}
              <span class="flag bad" title={t('colors.conflict', { n: conflict })}><Icon name="alert" size={14} /></span>
            {/if}
            <button class="icon-btn sm" title={skip ? t('colors.include') : t('colors.skip')} onclick={() => toggleSkip(row)}>
              <Icon name={skip ? 'eyeOff' : 'eye'} size={15} />
            </button>
          </div>
          {#if open === row.key && app.byColor && stored}
            <div class="body">
              <SettingsEditor settings={stored.settings} onchange={save} />
              <button class="btn test" title={stored.settings.tool === 'pen' ? t('test.buttonPenHint') : t('test.buttonHint')} onclick={() => app.openTestCut(stored.settings, row.color)}>
                <Icon name="testCut" size={15} />{stored.settings.tool === 'pen' ? t('test.buttonPen') : t('test.button')}
              </button>
              {#if row.passIndex > 0}
                <label class="switch pause">
                  <input
                    type="checkbox"
                    checked={row.pass?.toolChange || row.prefs.pauseBefore}
                    disabled={row.pass?.toolChange}
                    onchange={(e) => {
                      stored.pauseBefore = (e.target as HTMLInputElement).checked;
                      save();
                    }}
                  />
                  <span class="track"></span>{row.pass?.toolChange ? t(row.pass.settings.tool === 'pen' ? 'colors.toolChange.pen' : 'colors.toolChange.blade') : t('colors.pause')}
                </label>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if !app.byColor && rows.length}
    <div class="single">
      <div class="single-title">{t('colors.all')}</div>
      <SettingsEditor settings={app.single} onchange={save} />
      <button class="btn test" title={app.single.tool === 'pen' ? t('test.buttonPenHint') : t('test.buttonHint')} onclick={() => app.openTestCut(app.single, null)}>
        <Icon name="testCut" size={15} />{app.single.tool === 'pen' ? t('test.buttonPen') : t('test.button')}
      </button>
    </div>
  {/if}
</section>

<style>
  .count {
    padding: 0 7px;
    border-radius: 9px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    letter-spacing: 0;
  }

  .auto,
  .mirror {
    margin-top: 10px;
  }

  .explain {
    margin: 6px 0 0 39px;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 12px;
  }

  .row {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
    transition:
      box-shadow 0.12s,
      border-color 0.12s;
  }

  .row.open {
    border-color: var(--accent-line);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .row.dragging {
    box-shadow: var(--shadow-2);
    border-color: var(--accent-line);
    position: relative;
    z-index: 1;
  }

  .row.skip .swatch,
  .row.skip .name {
    opacity: 0.45;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 4px 6px 4px 6px;
  }

  .grip {
    display: grid;
    place-items: center;
    width: 16px;
    height: 28px;
    color: var(--text-3);
    cursor: grab;
    touch-action: none;
  }

  .grip:active {
    cursor: grabbing;
  }

  .num {
    display: grid;
    place-items: center;
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--panel-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    font-size: 11px;
    font-weight: 650;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 2px 0;
    border: 0;
    background: none;
    text-align: left;
  }

  .main:disabled {
    cursor: default;
  }

  .name {
    font-weight: 600;
  }

  .hex {
    margin-left: 7px;
    color: var(--text-3);
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 500;
  }

  .sub {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-3);
    font-size: 12px;
  }

  .flag {
    display: grid;
    place-items: center;
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 6px;
  }

  .flag.tool {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .flag.bad {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .body {
    padding: 4px 12px 12px;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }

  .pause {
    margin-top: 12px;
    font-size: 12.5px;
  }

  .test {
    margin-top: 12px;
    height: 28px;
    font-size: 12.5px;
  }

  .single {
    margin-top: 14px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .single-title {
    margin-bottom: 10px;
    font-weight: 600;
  }
</style>
