<script lang="ts">
  // Sending a job: choose where to, check the passes, then follow the machine: progress, pauses
  // for tool changes, pause and stop. The job runs on the Rust side; this only shows it.
  import { app, deviceKey } from '../lib/state.svelte';
  import { t, errorText } from '../lib/i18n/index.svelte';
  import { displayColor } from '../lib/theme.svelte';
  import { fmtLength } from '../lib/format';
  import { colorName as nameOf } from '../lib/colors';
  import { fromUnit, toUnit, unitLabel } from '../lib/format';
  import type { PassInfo, Settings } from '../lib/api/types';
  import Icon from './Icon.svelte';
  import NumberField from './NumberField.svelte';

  const s = $derived(app.send);
  const plan = $derived(s?.plan ?? app.plan);
  const running = $derived(!!s && ['connecting', 'running', 'paused', 'finishing', 'stopping'].includes(s.phase));
  const usb = $derived(app.target !== 'virtual');
  const testing = $derived(s?.mode === 'test');
  const canStart = $derived(!!plan && (testing ? !s?.error : !app.planError) && (!usb || app.understood));
  const percent = $derived(s && s.total > 0 ? Math.min(100, (s.sent / s.total) * 100) : 0);
  const passes = $derived(plan?.passes ?? []);
  const current = $derived(s && passes[s.pass] ? passes[s.pass] : null);

  function toolName(p: PassInfo | Settings): string {
    const tool = 'settings' in p ? p.settings.tool : p.tool;
    return tool === 'pen' ? t('tool.pen') : t('tool.blade');
  }

  function colorName(color: string | null): string {
    return nameOf(color);
  }

  function details(x: Settings): string {
    const parts: string[] = [];
    if (x.force !== null) parts.push(`${t('settings.force')} ${x.force}`);
    if (x.speed !== null) parts.push(`${t('settings.speed')} ${x.speed}`);
    if (x.tool === 'blade' && x.depth !== null) parts.push(`${t('settings.depth')} ${x.depth}`);
    if (x.repeats > 1) parts.push(`${x.repeats}×`);
    return parts.join(' · ') || t('settings.machineHint');
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === 'Escape' && !running) app.closeSend();
  }
</script>

<svelte:window onkeydown={onKey} />

{#if s}
  <div class="backdrop" class:side={testing} role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !running && app.closeSend()}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="send-title">
      <header>
        <div class="title-icon"><Icon name={testing ? 'testCut' : 'send'} size={18} /></div>
        <h2 id="send-title">{testing ? t('test.title') : t('send.title')}</h2>
        {#if !running}
          <button class="icon-btn" aria-label={t('send.close')} onclick={() => app.closeSend()}><Icon name="close" /></button>
        {/if}
      </header>

      {#if s.phase === 'setup'}
        <div class="content">
          {#if testing && app.test}
            <p class="lead">{t('test.body')}</p>
            <div class="label">{t('test.settings')}</div>
            <div class="test-settings">
              <span class="swatch" class:none={!app.test.color} style={app.test.color ? `background: ${displayColor(app.test.color)}` : ''}></span>
              <strong>{colorName(app.test.color)}</strong>
              <span class="p-tool"><Icon name={app.test.settings.tool === 'pen' ? 'pen' : 'blade'} size={13} />{toolName(app.test.settings)}</span>
              <span class="p-details">{details(app.test.settings)}</span>
            </div>
            <div class="label summary-label">{t('test.position')}</div>
            <div class="test-pos">
              <NumberField label="X" value={toUnit(app.test.x)} digits={1} unit={unitLabel()} onchange={(v) => app.moveTest(v === null ? null : fromUnit(v), null)} />
              <NumberField label="Y" value={toUnit(app.test.y)} digits={1} unit={unitLabel()} onchange={(v) => app.moveTest(null, v === null ? null : fromUnit(v))} />
              <button class="btn" onclick={() => app.nextTestSpot()}>{t('test.next')}</button>
            </div>
            {#if s.error}
              <div class="error-box"><Icon name="alert" size={15} />{t('test.outside')}</div>
            {/if}
            <div class="label summary-label">{t('send.target')}</div>
          {:else}
            <div class="label">{t('send.target')}</div>
          {/if}
          <div class="targets">
            <label class="target" class:on={app.target === 'virtual'}>
              <input type="radio" name="target" value="virtual" bind:group={app.target} />
              <span class="t-icon"><Icon name="virtual" size={18} /></span>
              <span class="t-text"><strong>{t('machine.virtual')}</strong><span>{t('send.virtualHint')}</span></span>
            </label>
            {#each app.devices as d (deviceKey(d))}
              <label class="target" class:on={app.target === deviceKey(d)}>
                <input type="radio" name="target" value={deviceKey(d)} bind:group={app.target} />
                <span class="t-icon"><Icon name={d.bluetooth ? 'bluetooth' : 'usb'} size={18} /></span>
                <span class="t-text"><strong>{d.name}</strong><span class="warn">{t(d.bluetooth ? 'send.bluetoothHint' : 'send.usbHint')}</span></span>
              </label>
            {/each}
          </div>
          {#if usb}
            <label class="understand">
              <input type="checkbox" bind:checked={app.understood} />
              <span>{t('send.understand')}</span>
            </label>
          {/if}

          {#if !testing}
          <div class="label summary-label">{t('send.summary')}</div>
          <div class="summary">
            <div class="machine">
              <strong>{app.machine?.name.replace(/^Silhouette /, '')}</strong>
              <span>· {app.mat?.id === 'none' ? t('mat.none') : app.mat?.id.replace('x', ' × ') + ' in'}</span>
              {#if plan}<span class="spacer"></span><span class="tnum">{fmtLength(plan.cutMm)}</span>{/if}
            </div>
            {#if plan}
              <ol class="passes">
                {#each plan.passes as p, i (i)}
                  <li>
                    {#if p.pauseBefore}
                      <div class="pause-line">
                        <Icon name={p.toolChange ? 'hand' : 'pause'} size={13} />
                        {p.toolChange ? t(p.settings.tool === 'pen' ? 'colors.toolChange.pen' : 'colors.toolChange.blade') : t('colors.pause')}
                      </div>
                    {/if}
                    <div class="pass">
                      <span class="num tnum">{i + 1}</span>
                      <span class="swatch" class:none={!p.color} style={p.color ? `background: ${displayColor(p.color)}` : ''}></span>
                      <span class="p-name">{colorName(p.color)}</span>
                      <span class="p-tool"><Icon name={p.settings.tool === 'pen' ? 'pen' : 'blade'} size={13} />{toolName(p)}</span>
                      <span class="p-details">{details(p.settings)}</span>
                    </div>
                  </li>
                {/each}
              </ol>
            {:else if app.planError}
              <div class="error-box"><Icon name="alert" size={15} />{t('send.noPlan', { reason: errorText(app.planError) })}</div>
            {/if}
          </div>
          {/if}
        </div>
        <footer>
          <button class="btn" onclick={() => app.closeSend()}>{t('send.cancel')}</button>
          <button class="btn primary lg" disabled={!canStart} onclick={() => app.startSend()}><Icon name="play" size={14} />{testing ? t('test.start') : t('send.start')}</button>
        </footer>
      {:else}
        <div class="content running">
          {#if s.phase === 'done' || s.phase === 'stopped' || s.phase === 'failed'}
            <div class="result {s.phase}">
              <div class="r-icon"><Icon name={s.phase === 'done' ? 'check' : s.phase === 'stopped' ? 'stop' : 'alert'} size={26} /></div>
              <h3>{s.phase === 'done' ? t('send.done') : s.phase === 'stopped' ? t('send.stopped') : t('send.failed')}</h3>
              <p>{s.phase === 'done' ? (testing ? t('test.doneBody') : t('send.doneBody')) : s.phase === 'stopped' ? t('send.stoppedBody') : errorText(s.error)}</p>
            </div>
          {:else if s.phase === 'paused' && s.pausedBefore !== null && current}
            <div class="change">
              <div class="c-icon"><Icon name={current.toolChange ? 'hand' : 'pause'} size={22} /></div>
              <div>
                <h3>{current.toolChange ? t('send.toolChange') : t('send.paused')}</h3>
                <p>
                  {current.toolChange
                    ? t(current.settings.tool === 'pen' ? 'send.toolChangeBody.pen' : 'send.toolChangeBody.blade', { n: s.pass + 1, color: colorName(current.color) })
                    : t('send.pauseBody', { n: s.pass + 1, color: colorName(current.color) })}
                </p>
              </div>
            </div>
          {:else}
            <div class="stage-line">
              {#if current && s.phase === 'running'}
                <span class="swatch" class:none={!current.color} style={current.color ? `background: ${displayColor(current.color)}` : ''}></span>
              {/if}
              <span class="stage-text">
                {#if s.phase === 'connecting'}{s.version ? t('send.connected', { version: s.version }) : t('send.connecting')}
                {:else if s.phase === 'running' && s.pausing}{t('send.pausing')}
                {:else if s.phase === 'running'}{passes.length > 1 ? t('send.sending', { n: s.pass + 1, total: passes.length }) : t('send.sendingOne')}
                {:else if s.phase === 'paused'}{t('send.paused')}
                {:else if s.phase === 'finishing'}{t('send.finishing')}
                {:else if s.phase === 'stopping'}{t('send.stopping')}{/if}
              </span>
              <span class="spacer"></span>
              <span class="tnum pct">{Math.round(percent)} %</span>
            </div>
          {/if}

          {#if s.phase !== 'failed'}
            <div class="progress" class:indeterminate={s.phase === 'connecting'}>
              <div class="fill" style="width: {s.phase === 'done' ? 100 : percent}%"></div>
            </div>
            {#if passes.length > 1}
              <div class="pass-dots">
                {#each passes as p, i (i)}
                  <span class="pd" class:done={i < s.pass || s.phase === 'done'} class:now={i === s.pass && s.phase !== 'done'} title="{i + 1}: {colorName(p.color)}">
                    <span class="swatch" class:none={!p.color} style={p.color ? `background: ${displayColor(p.color)}` : ''}></span>
                  </span>
                {/each}
              </div>
            {/if}
          {/if}
          {#if s.version && s.phase !== 'connecting'}<div class="hint version">{t('send.connected', { version: s.version })}</div>{/if}
        </div>
        <footer>
          {#if running}
            <button class="btn danger" disabled={s.phase === 'finishing' || s.phase === 'stopping'} onclick={() => app.control('stop')}><Icon name="stop" size={13} />{t('send.stop')}</button>
            <span class="spacer"></span>
            {#if s.phase === 'paused'}
              <button class="btn primary lg" onclick={() => app.control('resume')}><Icon name="play" size={14} />{t('send.resume')}</button>
            {:else}
              <button class="btn" disabled={s.phase !== 'running' || s.pausing} onclick={() => app.control('pause')}><Icon name="pause" size={13} />{t('send.pause')}</button>
            {/if}
          {:else}
            {#if testing}
              <button class="btn" onclick={() => { app.nextTestSpot(); void app.startSend(); }}><Icon name="testCut" size={15} />{t('test.again')}</button>
            {:else}
              <button class="btn" onclick={() => app.startSend()}><Icon name="restart" size={15} />{t('send.again')}</button>
            {/if}
            <span class="spacer"></span>
            <button class="btn primary" onclick={() => app.closeSend()}>{t('send.close')}</button>
          {/if}
        </footer>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    place-items: center;
    background: rgba(10, 16, 24, 0.42);
    backdrop-filter: blur(2px);
    animation: fade 0.15s ease-out;
  }

  .backdrop.side {
    place-items: center end;
    padding-right: 20px;
    background: rgba(10, 16, 24, 0.12);
    backdrop-filter: none;
  }

  .lead {
    margin: 0 0 14px;
    color: var(--text-2);
  }

  .test-settings {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 12.5px;
  }

  .test-pos {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    align-items: end;
    gap: 10px;
  }

  .dialog {
    width: min(520px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-3);
    animation: pop 0.18s ease-out;
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
    padding: 14px 20px 6px;
    overflow-y: auto;
  }

  .targets {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .target {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    cursor: pointer;
  }

  .target.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .target input {
    accent-color: var(--accent);
    margin: 0;
  }

  .t-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--panel-2);
    color: var(--text-2);
  }

  .t-text {
    display: flex;
    flex-direction: column;
    font-size: 12.5px;
  }

  .t-text span {
    color: var(--text-3);
  }

  .t-text .warn {
    color: var(--warn);
  }

  .understand {
    display: flex;
    gap: 9px;
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--warn-soft);
    color: var(--text);
    font-size: 12.5px;
    cursor: pointer;
  }

  .understand input {
    accent-color: var(--accent);
    margin-top: 2px;
  }

  .summary-label {
    margin-top: 16px;
  }

  .summary {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .machine {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 9px 12px;
    background: var(--panel-2);
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
  }

  .machine span {
    color: var(--text-2);
  }

  .spacer {
    flex: 1;
  }

  .passes {
    list-style: none;
    margin: 0;
    padding: 6px 0;
  }

  .pass {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 12px;
    font-size: 12.5px;
  }

  .pause-line {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 2px 12px;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 12px;
    font-weight: 550;
  }

  .num {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--panel-2);
    border: 1px solid var(--border);
    font-size: 11px;
    font-weight: 650;
    color: var(--text-2);
  }

  .p-name {
    font-weight: 600;
  }

  .p-tool {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-2);
  }

  .p-details {
    flex: 1;
    text-align: right;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .error-box {
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    color: var(--danger);
    font-size: 12.5px;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 20px 18px;
  }

  .running {
    padding-top: 18px;
    padding-bottom: 10px;
  }

  .stage-line {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 10px;
    font-weight: 600;
  }

  .pct {
    color: var(--text-2);
  }

  .progress {
    position: relative;
    height: 10px;
    border-radius: 5px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .progress .fill {
    height: 100%;
    border-radius: 5px;
    background: var(--accent);
    background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 70%, #60a5fa));
    transition: width 0.25s ease-out;
  }

  .progress.indeterminate .fill {
    width: 30% !important;
    animation: slide 1.2s ease-in-out infinite;
  }

  .pass-dots {
    display: flex;
    gap: 6px;
    margin-top: 10px;
  }

  .pd {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 1px solid var(--border);
    opacity: 0.45;
  }

  .pd.done {
    opacity: 0.8;
  }

  .pd.now {
    opacity: 1;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .version {
    margin-top: 12px;
    font-family: var(--mono);
    font-size: 11px;
  }

  .change {
    display: flex;
    gap: 14px;
    margin-bottom: 14px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    border: 1px solid var(--accent-line);
  }

  .change h3,
  .result h3 {
    margin: 0 0 4px;
    font-size: 15px;
  }

  .change p,
  .result p {
    margin: 0;
    color: var(--text-2);
  }

  .c-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
  }

  .result {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 6px 10px 16px;
  }

  .r-icon {
    display: grid;
    place-items: center;
    width: 54px;
    height: 54px;
    margin-bottom: 10px;
    border-radius: 50%;
    background: rgba(34, 160, 80, 0.16);
    background: color-mix(in srgb, var(--ok) 16%, transparent);
    color: var(--ok);
  }

  .result.stopped .r-icon {
    background: var(--warn-soft);
    color: var(--warn);
  }

  .result.failed .r-icon {
    background: var(--danger-soft);
    color: var(--danger);
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @keyframes pop {
    from {
      transform: translateY(6px) scale(0.98);
      opacity: 0;
    }
  }

  @keyframes slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(340%);
    }
  }
</style>
