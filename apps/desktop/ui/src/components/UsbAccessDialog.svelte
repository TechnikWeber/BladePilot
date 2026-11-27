<script lang="ts">
  // Linux without the udev rule (AppImage, build from source): offer to set it up, asking for the
  // password through pkexec, or show the command to paste into a terminal.
  import { app } from '../lib/state.svelte';
  import { t, errorText } from '../lib/i18n/index.svelte';
  import type { ApiError } from '../lib/api/types';
  import { backend } from '../lib/api';
  import Icon from './Icon.svelte';

  let busy = $state(false);
  let showCommand = $state(false);
  let error = $state<ApiError | null>(null);
  let copied = $state(false);
  let codeEl: HTMLElement | undefined = $state();

  const usb = $derived(app.usb);

  $effect(() => {
    if (usb && !usb.canElevate) showCommand = true;
  });

  async function install(): Promise<void> {
    busy = true;
    error = null;
    try {
      app.usb = await backend.installUsbAccess();
      if (app.usb.installed) {
        app.usbDialog = false;
        app.toast(t('usb.done'), 'info');
      }
    } catch (e) {
      error = e as ApiError;
      showCommand = true;
    } finally {
      busy = false;
    }
  }

  async function copy(): Promise<void> {
    if (!usb) return;
    try {
      await navigator.clipboard.writeText(usb.command);
    } catch {
      // No clipboard access: select the text so Ctrl+C copies it.
      const range = document.createRange();
      if (codeEl) range.selectNodeContents(codeEl);
      getSelection()?.removeAllRanges();
      getSelection()?.addRange(range);
    }
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  async function recheck(): Promise<void> {
    await app.checkUsbAccess();
    if (app.usb?.installed) {
      app.usbDialog = false;
      app.toast(t('usb.done'), 'info');
    }
  }
</script>

{#if app.usbDialog && usb}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="usb-title">
      <header>
        <div class="title-icon"><Icon name="usb" size={18} /></div>
        <h2 id="usb-title">{t('usb.title')}</h2>
        <button class="icon-btn" aria-label={t('send.close')} onclick={() => (app.usbDialog = false)}><Icon name="close" /></button>
      </header>
      <div class="content">
        <p>{t('usb.body')}</p>
        {#if error && error.code !== 'elevateDenied'}
          <div class="error-box"><Icon name="alert" size={15} />{errorText(error)}</div>
        {/if}
        {#if showCommand}
          <p class="hint">{t('usb.commandHint')}</p>
          <div class="code">
            <code bind:this={codeEl}>{usb.command}</code>
            <button class="btn copy" onclick={copy}><Icon name={copied ? 'check' : 'duplicate'} size={14} />{copied ? t('usb.copied') : t('usb.copy')}</button>
          </div>
          <p class="hint">{t('usb.replug')}</p>
        {/if}
      </div>
      <footer>
        <button class="btn ghost" onclick={() => app.usbNeverAsk()}>{t('usb.never')}</button>
        <span class="spacer"></span>
        {#if showCommand}
          <button class="btn" onclick={recheck}><Icon name="refresh" size={14} />{t('usb.recheck')}</button>
        {:else}
          <button class="btn" onclick={() => (showCommand = true)}>{t('usb.showCommand')}</button>
        {/if}
        {#if usb.canElevate}
          <button class="btn primary" disabled={busy} onclick={install}><Icon name="check" size={14} />{busy ? t('usb.working') : t('usb.install')}</button>
        {/if}
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
    width: min(580px, calc(100vw - 32px));
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
    padding: 10px 20px 4px;
  }

  .content p {
    margin: 0 0 12px;
    color: var(--text-2);
  }

  .code {
    position: relative;
    margin-bottom: 10px;
    padding: 12px 12px 44px;
    border-radius: var(--radius);
    background: var(--panel-2);
    border: 1px solid var(--border);
  }

  code {
    display: block;
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.5;
    word-break: break-all;
    user-select: text;
    -webkit-user-select: text;
  }

  .copy {
    position: absolute;
    right: 8px;
    bottom: 8px;
    height: 28px;
  }

  .error-box {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
    color: var(--danger);
    font-size: 12.5px;
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
