<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { t, errorText } from '../lib/i18n/index.svelte';
  import { fmtLength } from '../lib/format';
  import { backend } from '../lib/api';
  import Icon from './Icon.svelte';

  function setUnit(unit: 'mm' | 'in'): void {
    app.unit = unit;
    app.savePrefs();
  }
</script>

<footer class="status">
  {#if app.items.length}
    <span>{app.items.length === 1 ? t('status.design') : t('status.designs', { n: app.items.length })}</span>
    {#if app.planError}
      <span class="error"><Icon name="alert" size={14} />{errorText(app.planError)}</span>
    {:else if app.plan}
      <span class="dot">·</span>
      <span>{app.plan.passes.length === 1 ? t('status.pass') : t('status.passes', { n: app.plan.passes.length })}</span>
      <span class="dot">·</span>
      <span class="tnum">{t('status.cut', { len: fmtLength(app.plan.cutMm) })}</span>
      <span class="dot">·</span>
      <span class="tnum muted">{t('status.travel', { len: fmtLength(app.plan.travelMm) })}</span>
      {#if app.planning}<span class="muted">· {t('status.planning')}</span>{:else}<span class="ok"><Icon name="check" size={13} />{t('status.ready')}</span>{/if}
    {:else if app.planning}
      <span class="muted">{t('status.planning')}</span>
    {/if}
  {/if}
  <span class="spacer"></span>
  {#if backend.mock}<span class="mock" title={t('status.mock')}><Icon name="info" size={13} />{t('status.mock')}</span>{/if}
  <span class="muted">{t('status.unit')}</span>
  <div class="segmented units">
    <button class:on={app.unit === 'mm'} onclick={() => setUnit('mm')}>mm</button>
    <button class:on={app.unit === 'in'} onclick={() => setUnit('in')}>{t('units.in')}</button>
  </div>
  {#if app.info}<span class="muted version">v{app.info.version}</span>{/if}
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px 0 14px;
    background: var(--panel);
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
  }

  .spacer {
    flex: 1;
  }

  .dot,
  .muted {
    color: var(--text-3);
  }

  .error {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 6px;
    color: var(--danger);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ok {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 6px;
    color: var(--ok);
    font-weight: 600;
  }

  .mock {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-right: 8px;
    color: var(--text-3);
  }

  .units button {
    height: 20px;
    padding: 0 8px;
    font-size: 11.5px;
  }

  .version {
    margin-left: 4px;
  }
</style>
