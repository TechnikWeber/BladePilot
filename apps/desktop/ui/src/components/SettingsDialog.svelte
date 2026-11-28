<script lang="ts">
  // Settings: units and the mat's grid, appearance and language, where test cuts start, the
  // chosen machine's corrections (from the alignment test and the calibration sheet), and the
  // diagnosis for bug reports.
  import { app, type TestCorner } from '../lib/state.svelte';
  import { t, setLanguage, errorText } from '../lib/i18n/index.svelte';
  import { setTheme } from '../lib/theme.svelte';
  import { backend } from '../lib/api';
  import NumberField from './NumberField.svelte';
  import Icon from './Icon.svelte';

  const CORNERS: { id: TestCorner; arrow: string; key: 'test.cornerTl' | 'test.cornerTr' | 'test.cornerBl' | 'test.cornerBr' }[] = [
    { id: 'tl', arrow: '↖', key: 'test.cornerTl' },
    { id: 'tr', arrow: '↗', key: 'test.cornerTr' },
    { id: 'bl', arrow: '↙', key: 'test.cornerBl' },
    { id: 'br', arrow: '↘', key: 'test.cornerBr' },
  ];

  let copied = $state(false);

  const correction = $derived(app.corrections[app.machineId] ?? { mat: [0, 0] as [number, number], print: [0, 0] as [number, number] });
  const machineName = $derived(app.machine?.name.replace(/^Silhouette /, '') ?? '');

  function close(): void {
    app.settingsOpen = false;
  }

  function setUnit(unit: 'mm' | 'in'): void {
    app.unit = unit;
    app.savePrefs();
  }

  function setScroll(mode: 'pan' | 'zoom'): void {
    app.scrollMode = mode;
    app.savePrefs();
  }

  function setGrid(grid: 'auto' | 'cm' | 'in'): void {
    app.gridUnit = grid;
    app.savePrefs();
  }

  function chooseTheme(choice: 'system' | 'light' | 'dark'): void {
    app.themeChoice = choice;
    setTheme(choice);
    app.savePrefs();
  }

  function chooseLanguage(choice: 'system' | 'de' | 'en'): void {
    app.languageChoice = choice;
    setLanguage(choice === 'system' ? (app.info?.language ?? navigator.language) : choice);
    app.savePrefs();
  }

  function setOffset(kind: 'mat' | 'print', axis: 0 | 1, value: number | null): void {
    const v: [number, number] = [...correction[kind]];
    v[axis] = Math.max(-10, Math.min(10, value ?? 0));
    app.setCorrection(kind, v);
  }

  async function saveDiagnosis(): Promise<void> {
    try {
      const text = await backend.diagnostics();
      const path = await backend.saveText(text, 'bladepilot-diagnosis.txt');
      if (path) app.toast(t('diag.saved', { path }), 'info');
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  async function copyDiagnosis(): Promise<void> {
    try {
      await navigator.clipboard.writeText(await backend.diagnostics());
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === 'Escape') close();
  }
</script>

{#if app.settingsOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" role="presentation" onkeydown={onKey} onpointerdown={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <header>
        <div class="title-icon"><Icon name="settings" size={18} /></div>
        <h2 id="settings-title">{t('settings.title')}</h2>
        <button class="icon-btn" aria-label={t('send.close')} onclick={close}><Icon name="close" /></button>
      </header>
      <div class="content">
        <section>
          <h3>{t('settings.view')}</h3>
          <div class="row">
            <span>{t('settings.units')}</span>
            <div class="segmented">
              <button class:on={app.unit === 'mm'} onclick={() => setUnit('mm')}>mm</button>
              <button class:on={app.unit === 'in'} onclick={() => setUnit('in')}>{t('units.in')}</button>
            </div>
          </div>
          <div class="row">
            <span>{t('settings.grid')}</span>
            <div class="segmented">
              <button class:on={app.gridUnit === 'auto'} onclick={() => setGrid('auto')}>{t('settings.gridAuto')}</button>
              <button class:on={app.gridUnit === 'cm'} onclick={() => setGrid('cm')}>cm</button>
              <button class:on={app.gridUnit === 'in'} onclick={() => setGrid('in')}>{t('units.in')}</button>
            </div>
          </div>
          <p class="hint">{t('settings.gridHint')}</p>
          <div class="row">
            <span>{t('settings.scroll')}</span>
            <div class="segmented">
              <button class:on={app.scrollMode === 'pan'} onclick={() => setScroll('pan')}>{t('settings.scrollPan')}</button>
              <button class:on={app.scrollMode === 'zoom'} onclick={() => setScroll('zoom')}>{t('settings.scrollZoom')}</button>
            </div>
          </div>
          <p class="hint">{app.scrollMode === 'pan' ? t('settings.scrollPanHint') : t('settings.scrollZoomHint')}</p>
          <div class="row">
            <span>{t('settings.theme')}</span>
            <div class="segmented">
              <button class:on={app.themeChoice === 'system'} onclick={() => chooseTheme('system')}>{t('settings.system')}</button>
              <button class:on={app.themeChoice === 'light'} onclick={() => chooseTheme('light')}>{t('settings.light')}</button>
              <button class:on={app.themeChoice === 'dark'} onclick={() => chooseTheme('dark')}>{t('settings.dark')}</button>
            </div>
          </div>
          <div class="row">
            <span>{t('settings.language')}</span>
            <div class="segmented">
              <button class:on={app.languageChoice === 'system'} onclick={() => chooseLanguage('system')}>{t('settings.system')}</button>
              <button class:on={app.languageChoice === 'de'} onclick={() => chooseLanguage('de')}>Deutsch</button>
              <button class:on={app.languageChoice === 'en'} onclick={() => chooseLanguage('en')}>English</button>
            </div>
          </div>
        </section>

        <section>
          <h3>{t('settings.testCut')}</h3>
          <div class="row">
            <span>{t('test.corner')}</span>
            <div class="segmented corners">
              {#each CORNERS as c (c.id)}
                <button class:on={app.testCorner === c.id} title={t(c.key)} aria-label={t(c.key)} onclick={() => app.setTestCorner(c.id)}>{c.arrow}</button>
              {/each}
            </div>
          </div>
        </section>

        <section>
          <h3>{t('settings.correction', { machine: machineName })}</h3>
          <p class="hint">{t('settings.correctionHint')}</p>
          <div class="offsets">
            <span class="kind">{t('settings.correctionMat')}</span>
            <NumberField label={t('settings.right')} value={correction.mat[0]} digits={1} step={0.5} min={-10} max={10} unit="mm" onchange={(v) => setOffset('mat', 0, v)} />
            <NumberField label={t('settings.down')} value={correction.mat[1]} digits={1} step={0.5} min={-10} max={10} unit="mm" onchange={(v) => setOffset('mat', 1, v)} />
            <button class="btn" onclick={() => { close(); void app.openAlignment(); }}><Icon name="testCut" size={15} />{t('align.button')}</button>
          </div>
          <div class="offsets">
            <span class="kind">{t('settings.correctionPrint')}</span>
            <NumberField label={t('settings.right')} value={correction.print[0]} digits={1} step={0.5} min={-10} max={10} unit="mm" onchange={(v) => setOffset('print', 0, v)} />
            <NumberField label={t('settings.down')} value={correction.print[1]} digits={1} step={0.5} min={-10} max={10} unit="mm" onchange={(v) => setOffset('print', 1, v)} />
            <button class="btn" onclick={() => { close(); void app.openCalibration(); }}><Icon name="printer" size={15} />{t('calib.button')}</button>
          </div>
        </section>

        <section>
          <h3>{t('settings.help')}</h3>
          <p class="hint">{t('diag.hint')}</p>
          <div class="buttons">
            <button class="btn" onclick={saveDiagnosis}><Icon name="info" size={15} />{t('diag.save')}</button>
            <button class="btn ghost" onclick={copyDiagnosis}><Icon name={copied ? 'check' : 'duplicate'} size={15} />{copied ? t('usb.copied') : t('diag.copy')}</button>
          </div>
        </section>
      </div>
      <footer>
        {#if app.info}<span class="version">BladePilot {app.info.version}</span>{/if}
        <span class="spacer"></span>
        <button class="btn primary" onclick={close}>{t('settings.done')}</button>
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
    display: flex;
    flex-direction: column;
    width: min(600px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
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

  .content {
    overflow-y: auto;
    padding: 4px 20px 8px;
  }

  section {
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }

  section:last-child {
    border-bottom: 0;
  }

  h3 {
    margin: 0 0 10px;
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-2);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
    font-size: 13px;
  }

  .hint {
    margin: -2px 0 10px;
    color: var(--text-2);
    font-size: 12px;
  }

  .corners button {
    min-width: 34px;
    justify-content: center;
    font-size: 15px;
  }

  .offsets {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 130px 130px;
    align-items: end;
    gap: 8px;
    margin-bottom: 14px;
  }

  .offsets .btn {
    grid-column: 1 / -1;
    justify-self: start;
  }

  .offsets .kind {
    align-self: center;
    font-size: 12.5px;
    font-weight: 600;
  }

  @media (max-width: 560px) {
    .offsets {
      grid-template-columns: 1fr 1fr;
    }

    .offsets .kind {
      grid-column: 1 / -1;
    }
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px 16px;
    border-top: 1px solid var(--border);
  }

  .version {
    color: var(--text-3);
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }
</style>
