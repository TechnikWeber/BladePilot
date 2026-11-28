<script lang="ts">
  // Print and cut: the sheet with the designs and the registration marks, then the cut.
  import { app } from '../lib/state.svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { fromUnit, toUnit, unitLabel } from '../lib/format';
  import NumberField from './NumberField.svelte';
  import Icon from './Icon.svelte';

  function setPrint(on: boolean): void {
    app.printMode = on;
    // Pictures put on before default to their contour once printing is on.
    if (on) for (const item of app.items) if (app.designs[item.design]?.image && item.cut.kind === 'lines') item.cut = { kind: 'contour', offsetMm: 2 };
    app.savePrefs();
  }
</script>

<section class="panel-section">
  <h3 class="panel-title">{t('print.title')}</h3>
  <label class="switch">
    <input type="checkbox" checked={app.printMode} onchange={(e) => setPrint((e.target as HTMLInputElement).checked)} />
    <span class="track"></span>{t('print.on')}
  </label>
  {#if app.printMode}
    <div class="rows">
      <label>
        <span class="label">{t('print.paper')}</span>
        <select
          class="select"
          value={app.printSetup.paper}
          onchange={(e) => {
            app.printSetup.paper = (e.target as HTMLSelectElement).value as 'a4' | 'letter';
            app.savePrefs();
          }}
        >
          <option value="a4">{t('print.a4')}</option>
          <option value="letter">{t('print.letter')}</option>
        </select>
      </label>
      <NumberField
        label={t('print.inset')}
        value={toUnit(app.printSetup.insetMm)}
        digits={app.unit === 'in' ? 2 : 0}
        min={0}
        unit={unitLabel()}
        onchange={(v) => {
          if (v !== null) app.printSetup.insetMm = Math.max(5, Math.min(40, fromUnit(v)));
          app.savePrefs();
        }}
      />
    </div>
    {#if !app.marks}
      <div class="note warn"><Icon name="alert" size={15} />{t('print.noRoom')}</div>
    {/if}
    <button class="btn save" disabled={!app.job || !app.marks || !!app.planError} onclick={() => app.savePrintSheet()}>
      <Icon name="printer" size={16} />{t('print.save')}
    </button>
    <p class="hint">{t('print.hint')}</p>
    <button class="btn ghost calibrate" onclick={() => app.openCalibration()}><Icon name="testCut" size={14} />{t('calib.button')}</button>
  {/if}
</section>

<style>
  .rows {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 12px;
  }

  .save {
    width: 100%;
    margin-top: 12px;
  }

  .hint {
    margin: 8px 0 0;
  }

  .calibrate {
    margin-top: 8px;
  }

  .note {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--warn-soft);
    color: var(--warn);
    font-size: 12px;
  }
</style>
