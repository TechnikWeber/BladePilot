<script lang="ts">
  // The toolbar: the file menu (new, open, save, recent projects, settings), adding text and
  // shapes, undo, the editing tools and the zoom. Preview and Send sit under the machine status.
  import { app } from '../lib/state.svelte';
  import { t, num } from '../lib/i18n/index.svelte';
  import { backend } from '../lib/api';
  import Icon from './Icon.svelte';

  const hasSelection = $derived(app.selected !== null || app.selection.length > 0);
  let menuOpen = $state(false);
  let menuEl: HTMLDivElement | undefined = $state();

  function run(action: () => void | Promise<void>): void {
    menuOpen = false;
    void action();
  }

  function onWindowDown(e: PointerEvent): void {
    if (menuOpen && menuEl && !menuEl.contains(e.target as Node)) menuOpen = false;
  }

  function fileName(path: string): string {
    return (path.split(/[\\/]/).pop() ?? path).replace(/\.bladepilot$/i, '');
  }
</script>

<svelte:window onpointerdown={onWindowDown} onkeydown={(e) => e.key === 'Escape' && (menuOpen = false)} />

<header class="top">
  <div class="brand" title={app.projectPath ?? ''}>
    <img src="./favicon.png" alt="" width="26" height="26" />
    <span class="name">BladePilot</span>
    {#if app.projectName}<span class="project">· {app.projectName}{app.dirty ? ' •' : ''}</span>{/if}
  </div>

  <div class="group menu-wrap" bind:this={menuEl}>
    <button class="icon-btn" class:active={menuOpen} title={t('menu.title')} aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>
      <Icon name="menu" />
    </button>
    {#if menuOpen}
      <div class="menu" role="menu">
        <button role="menuitem" onclick={() => run(() => app.newProject())}><Icon name="newFile" size={15} />{t('menu.new')}<kbd>Ctrl+N</kbd></button>
        <button role="menuitem" onclick={() => run(() => app.open(() => backend.pickDesigns()))}><Icon name="open" size={15} />{t('menu.openDesign')}<kbd>Ctrl+O</kbd></button>
        <button role="menuitem" onclick={() => run(() => app.openProjectDialog())}><Icon name="folder" size={15} />{t('menu.openProject')}</button>
        {#if app.recent.length}
          <div class="menu-label">{t('menu.recent')}</div>
          {#each app.recent.slice(0, 5) as path (path)}
            <button role="menuitem" class="recent" title={path} onclick={() => run(() => app.openProjectPath(path))}>{fileName(path)}</button>
          {/each}
        {/if}
        <div class="menu-sep"></div>
        <button role="menuitem" disabled={!app.items.length} onclick={() => run(() => app.saveProject())}><Icon name="save" size={15} />{t('menu.save')}<kbd>Ctrl+S</kbd></button>
        <button role="menuitem" disabled={!app.items.length} onclick={() => run(() => app.saveProject(true))}><Icon name="save" size={15} />{t('menu.saveAs')}<kbd>Ctrl+⇧+S</kbd></button>
        <div class="menu-sep"></div>
        <button role="menuitem" onclick={() => run(() => app.openDemo())}><Icon name="sparkle" size={15} />{t('top.demo')}</button>
        <button role="menuitem" onclick={() => run(() => { app.settingsOpen = true; })}><Icon name="settings" size={15} />{t('settings.title')}</button>
      </div>
    {/if}
    <button class="btn" title={t('top.openHint')} onclick={() => app.open(() => backend.pickDesigns())}><Icon name="open" size={16} /><span class="txt">{t('top.open')}</span></button>
  </div>

  <div class="sep"></div>

  <div class="group">
    <button class="btn" title={t('top.textHint')} onclick={() => app.openMake('text')}><Icon name="text" size={16} /><span class="txt">{t('top.text')}</span></button>
    <button class="btn" title={t('top.shapeHint')} onclick={() => app.openMake('shape')}><Icon name="shapes" size={16} /><span class="txt">{t('top.shape')}</span></button>
  </div>

  <div class="sep"></div>

  <div class="group">
    <button class="icon-btn" title={t('top.undo')} disabled={!app.canUndo} onclick={() => app.undo()}><Icon name="undo" /></button>
    <button class="icon-btn" title={t('top.redo')} disabled={!app.canRedo} onclick={() => app.redo()}><Icon name="redo" /></button>
  </div>

  <div class="sep"></div>

  <div class="group tools">
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

  <button class="icon-btn" title={t('settings.title')} onclick={() => (app.settingsOpen = true)}><Icon name="settings" /></button>
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
    min-width: 0;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-right: 6px;
    min-width: 0;
    font-weight: 700;
    font-size: 15px;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .brand img {
    border-radius: 7px;
  }

  .project {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
    color: var(--text-2);
    font-weight: 550;
    font-size: 13px;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .group .btn + .btn {
    margin-left: 6px;
  }

  .menu-wrap {
    position: relative;
    gap: 6px;
  }

  .menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    min-width: 250px;
    padding: 6px;
    border-radius: 10px;
    background: var(--panel);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-3);
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    text-align: left;
  }

  .menu button:hover:not(:disabled) {
    background: var(--accent-soft);
  }

  .menu button:disabled {
    color: var(--text-3);
  }

  .menu kbd {
    margin-left: auto;
    color: var(--text-3);
    font-family: var(--mono);
    font-size: 11px;
  }

  .menu .recent {
    padding-left: 35px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }

  .menu-label {
    padding: 8px 10px 2px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-3);
  }

  .menu-sep {
    height: 1px;
    margin: 5px 4px;
    background: var(--border);
  }

  .sep {
    flex: none;
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

  .icon-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
  }

  /* Narrow windows: icons only, then fewer of them. */
  @media (max-width: 1180px) {
    .txt,
    .project {
      display: none;
    }

    .btn {
      padding: 0 8px;
    }
  }

  @media (max-width: 960px) {
    .brand .name,
    .level {
      display: none;
    }
  }

  @media (max-width: 820px) {
    .tools .icon-btn:nth-child(n + 3) {
      display: none;
    }
  }
</style>
