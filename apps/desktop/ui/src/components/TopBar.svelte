<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { t, num } from '../lib/i18n/index.svelte';
  import { backend } from '../lib/api';
  import Icon from './Icon.svelte';

  const hasSelection = $derived(app.selected !== null);
  const canCut = $derived(!!app.job && !!app.plan && !app.planError);
</script>

<header class="top">
  <div class="brand">
    <img src="./favicon.png" alt="" width="26" height="26" />
    <span>BladePilot</span>
  </div>

  <div class="group">
    <button class="btn" title={t('top.openHint')} onclick={() => app.open(() => backend.pickDesigns())}><Icon name="open" size={16} />{t('top.open')}</button>
    <button class="icon-btn" title={t('top.demo')} onclick={() => app.openDemo()}><Icon name="sparkle" /></button>
  </div>

  <div class="sep"></div>

  <div class="group">
    <button class="btn" title={t('top.textHint')} onclick={() => app.openMake('text')}><Icon name="text" size={16} />{t('top.text')}</button>
    <button class="btn" title={t('top.shapeHint')} onclick={() => app.openMake('shape')}><Icon name="shapes" size={16} />{t('top.shape')}</button>
  </div>

  <div class="sep"></div>

  <div class="group">
    <button class="icon-btn" title={t('top.undo')} disabled={!app.canUndo} onclick={() => app.undo()}><Icon name="undo" /></button>
    <button class="icon-btn" title={t('top.redo')} disabled={!app.canRedo} onclick={() => app.redo()}><Icon name="redo" /></button>
  </div>

  <div class="sep"></div>

  <div class="group">
    <button class="icon-btn" title={t('top.duplicate')} disabled={!hasSelection} onclick={() => app.duplicateSelected()}><Icon name="duplicate" /></button>
    <button class="icon-btn" title={t('top.rotate')} disabled={!hasSelection} onclick={() => app.rotateSelected(90)}><Icon name="rotate" /></button>
    <button class="icon-btn" title={t('top.mirrorH')} disabled={!hasSelection} onclick={() => app.mirrorSelected(true)}><Icon name="mirrorH" /></button>
    <button class="icon-btn" title={t('top.mirrorV')} disabled={!hasSelection} onclick={() => app.mirrorSelected(false)}><Icon name="mirrorV" /></button>
    <button class="icon-btn" title={t('top.delete')} disabled={!hasSelection} onclick={() => app.removeSelected()}><Icon name="trash" /></button>
    <button class="icon-btn" title={t('top.arrange')} disabled={!app.items.length} onclick={() => app.arrange()}><Icon name="arrange" /></button>
  </div>

  <div class="spacer"></div>

  <div class="group zoom">
    <button class="icon-btn" title={t('top.zoomOut')} onclick={() => app.zoomBy(1 / 1.25)}><Icon name="zoomOut" /></button>
    <span class="tnum level">{num(app.view.scale * (25.4 / 96) * 100, 0)} %</span>
    <button class="icon-btn" title={t('top.zoomIn')} onclick={() => app.zoomBy(1.25)}><Icon name="zoomIn" /></button>
    <button class="icon-btn" title={t('top.fit')} onclick={() => app.fitRequest++}><Icon name="fit" /></button>
  </div>

  <div class="sep"></div>

  <div class="group">
    <button class="btn" class:active={!!app.preview} title={t('top.previewHint')} disabled={!canCut} onclick={() => (app.preview ? app.closePreview() : app.openPreview())}>
      <Icon name="play" size={14} />{t('top.preview')}
    </button>
    <button class="btn primary" title={t('top.sendHint')} disabled={!canCut} onclick={() => app.openSend()}><Icon name="send" size={16} />{t('top.send')}</button>
  </div>
</header>

<style>
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 12px 0 14px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-right: 6px;
    font-weight: 700;
    font-size: 15px;
    letter-spacing: -0.01em;
  }

  .brand img {
    border-radius: 7px;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .group .btn + .btn {
    margin-left: 6px;
  }

  .sep {
    width: 1px;
    height: 22px;
    background: var(--border);
  }

  .spacer {
    flex: 1;
  }

  .level {
    min-width: 46px;
    text-align: center;
    color: var(--text-2);
    font-size: 12px;
  }

  .btn.active {
    background: var(--accent-soft);
    border-color: var(--accent-line);
    color: var(--accent);
  }
</style>
