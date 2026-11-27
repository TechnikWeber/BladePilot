<script lang="ts">
  import { app, deviceKey } from '../lib/state.svelte';
  import { t, num, errorText } from '../lib/i18n/index.svelte';
  import { fmtLength } from '../lib/format';
  import type { MatInfo } from '../lib/api/types';
  import Icon from './Icon.svelte';

  const targets = $derived(app.machines.filter((m) => m.target));
  const others = $derived(app.machines.filter((m) => !m.target));

  function matName(mat: MatInfo): string {
    if (mat.id === 'none') return t('mat.none');
    const [w, h] = mat.id.split('x').map(Number);
    return t('mat.size', { w: num(w, 1), h: num(h, 1) });
  }

  // Ask the machine (or the virtual plotter) how it is whenever the choice changes.
  $effect(() => {
    void app.target;
    void app.machineId;
    void app.checkStatus();
  });

  const found = $derived(app.device);
  const mismatch = $derived(found && found.machine && found.machine !== app.machineId ? found : null);

  const statusText = $derived.by(() => {
    if (app.checking) return t('machine.status.checking');
    if (app.statusError) return errorText(app.statusError);
    if (!app.status) return t('machine.status.offline');
    return t(`machine.status.${app.status.status}`);
  });
  const statusKind = $derived(app.checking ? 'busy' : app.statusError ? 'bad' : app.status?.status === 'ready' ? 'ok' : app.status ? 'warn' : 'off');
  const toolText = $derived.by(() => {
    const tool = app.status?.tools[0];
    if (!tool) return '';
    return tool === 'pen' ? t('settings.pen') : tool === 'empty' || tool === 'unknown' ? '' : tool === 'autoBlade' ? 'AutoBlade' : t('settings.blade');
  });
</script>

<section class="panel-section">
  <h3 class="panel-title">{t('machine.title')}</h3>
  <div class="rows">
    <label>
      <span class="label">{t('machine.model')}</span>
      <select class="select" value={app.machineId} onchange={(e) => app.setMachine((e.target as HTMLSelectElement).value)}>
        {#each targets as m (m.id)}<option value={m.id}>{m.name.replace(/^Silhouette /, '')}</option>{/each}
        <optgroup label={t('machine.untested')}>
          {#each others as m (m.id)}<option value={m.id}>{m.name.replace(/^Silhouette /, '')}</option>{/each}
        </optgroup>
      </select>
    </label>
    <label>
      <span class="label">{t('machine.mat')}</span>
      <select class="select" value={app.matId} onchange={(e) => app.setMat((e.target as HTMLSelectElement).value)}>
        {#each app.machine?.mats ?? [] as mat (mat.id)}<option value={mat.id}>{matName(mat)}</option>{/each}
      </select>
    </label>
  </div>
  {#if app.mat}
    <div class="hint area">{t('mat.area', { w: fmtLength(app.mat.areaWidthMm), h: app.mat.id === 'none' ? '∞' : fmtLength(app.mat.areaHeightMm) })}</div>
  {/if}

  <span class="label conn-label">{t('machine.connection')}</span>
  <div class="conn">
    <span class="conn-icon"><Icon name={found ? 'usb' : 'virtual'} size={16} /></span>
    <select class="select" bind:value={app.target}>
      <option value="virtual">{t('machine.virtual')}</option>
      {#each app.devices as d (deviceKey(d))}<option value={deviceKey(d)}>{d.name} · USB {d.bus}:{d.address}</option>{/each}
    </select>
    <button class="icon-btn" title={t('machine.search')} disabled={app.scanning} onclick={() => app.scan()}>
      <Icon name="refresh" class={app.scanning ? 'spin' : ''} />
    </button>
  </div>
  <div class="status">
    <span class="dot {statusKind}"></span>
    <span class="status-text">{statusText}</span>
    {#if app.status?.version}<span class="version" title={app.status.version}>{app.status.version}</span>{/if}
    {#if toolText}<span class="chip">{toolText}</span>{/if}
    <button class="icon-btn sm" title={t('machine.refresh')} disabled={app.checking} onclick={() => app.checkStatus()}><Icon name="refresh" size={14} /></button>
  </div>
  {#if mismatch}
    <div class="note warn"><Icon name="info" size={15} />{t('machine.otherModel', { found: mismatch.name })}</div>
  {/if}
  {#if found}
    <div class="note warn"><Icon name="alert" size={15} />{t('machine.experimental')}</div>
  {/if}
</section>

<style>
  .rows {
    display: grid;
    grid-template-columns: 1.15fr 1fr;
    gap: 10px;
  }

  .area {
    margin-top: 6px;
  }

  .conn-label {
    margin-top: 14px;
  }

  .conn {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .conn-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    padding: 7px 6px 7px 10px;
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    border: 1px solid var(--border);
    font-size: 12.5px;
  }

  .status-text {
    font-weight: 600;
    white-space: nowrap;
  }

  .version {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-3);
    font-family: var(--mono);
    font-size: 11px;
  }

  .chip {
    padding: 1px 7px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
  }

  .dot.ok {
    background: var(--ok);
    box-shadow: 0 0 0 3px rgba(34, 160, 80, 0.22);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 22%, transparent);
  }

  .dot.warn {
    background: var(--warn);
  }

  .dot.bad {
    background: var(--danger);
  }

  .dot.busy {
    background: var(--accent);
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  .note {
    display: flex;
    gap: 8px;
    margin-top: 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 1.35;
  }

  .note.warn {
    background: var(--warn-soft);
    color: var(--warn);
  }

  .note :global(.icon) {
    margin-top: 1px;
  }

  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
