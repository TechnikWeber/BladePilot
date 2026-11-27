<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { app } from './lib/state.svelte';
  import { backend } from './lib/api';
  import { setLanguage } from './lib/i18n/index.svelte';
  import TopBar from './components/TopBar.svelte';
  import MatView from './components/MatView.svelte';
  import MachinePanel from './components/MachinePanel.svelte';
  import ColorList from './components/ColorList.svelte';
  import ItemPanel from './components/ItemPanel.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import PreviewBar from './components/PreviewBar.svelte';
  import SendDialog from './components/SendDialog.svelte';
  import Toasts from './components/Toasts.svelte';

  let ready = $state(false);

  onMount(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      // In development, VITE_STATE adds the same switches inside the desktop window.
      const extra = import.meta.env.DEV ? (import.meta.env.VITE_STATE ?? '') : '';
      const params = new URLSearchParams(location.search + (extra ? `&${extra}` : ''));
      await app.init();
      setLanguage(params.get('lang') ?? app.info?.language ?? navigator.language);
      ready = true;
      unlisten = await backend.onFileDrop(
        async (paths) => {
          for (const path of paths) await app.open(async () => [await backend.openPath(path)], path);
        },
        (over) => (app.dropping = over),
      );
      if (!backend.mock) void app.scan();
      for (const path of await backend.startupFiles()) await app.open(async () => [await backend.openPath(path)], path);
      await screenshotState(params);
    })();
    return () => unlisten?.();
  });

  /** `?demo`, `?preview=0.6`, `?send[=change|start]`, `?test`, `?select`: states for screenshots and tests. */
  async function screenshotState(params: URLSearchParams): Promise<void> {
    if (params.has('machine')) app.setMachine(params.get('machine') as string);
    if (!params.has('demo')) return;
    await app.openDemo();
    if (!params.has('select')) app.selected = null;
    else app.selected = app.items[0]?.id ?? null;
    if (params.has('preview') || params.has('send')) {
      await app.replan(app.job!);
    }
    if (params.has('preview')) {
      await app.openPreview();
      const at = Number(params.get('preview') || '0.5');
      if (app.preview && app.timeline) {
        app.preview.playing = false;
        app.preview.time = app.timeline.total * at;
      }
    }
    if (params.has('test')) {
      const black = '#1f2328';
      await app.openTestCut(app.prefsOf(black).settings, black);
    }
    if (params.has('send')) {
      app.openSend();
      const phase = params.get('send');
      if (phase === 'start') await app.startSend();
      if (phase === 'change' && app.send) {
        app.send.phase = 'paused';
        app.send.pass = 1;
        app.send.pausedBefore = 1;
        app.send.sent = Math.round((app.plan?.bytes ?? 0) * 0.18);
        app.send.total = app.plan?.bytes ?? 0;
        app.send.version = 'PORTRAIT 3 V0.00 (virtual)';
      }
    }
  }

  // Plan again whenever the job changes, a moment after the last change and never mid-drag.
  $effect(() => {
    const job = app.job;
    const dragging = app.dragging;
    if (!ready) return;
    if (!job) {
      app.clearPlan();
      return;
    }
    const snapshot = JSON.stringify(job);
    if (dragging) return;
    const timer = setTimeout(() => void app.replan(JSON.parse(snapshot)), 120);
    return () => clearTimeout(timer);
  });

  // A changed job invalidates the preview.
  $effect(() => {
    void JSON.stringify(app.job);
    untrack(() => {
      if (app.preview && !app.preview.loading) app.closePreview();
    });
  });

  function onKey(e: KeyboardEvent): void {
    const target = e.target as HTMLElement;
    if (target.closest('input, select, textarea') || app.send) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const step = e.shiftKey ? 10 : 1;
    if (ctrl && e.key.toLowerCase() === 'o') {
      e.preventDefault();
      void app.open(() => backend.pickDesigns());
    } else if (ctrl && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey) app.redo();
      else app.undo();
    } else if (ctrl && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      app.redo();
    } else if (ctrl && e.key.toLowerCase() === 'd') {
      e.preventDefault();
      app.duplicateSelected();
    } else if (ctrl && e.key === 'Enter') {
      e.preventDefault();
      if (app.plan && !app.planError) app.openSend();
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      app.removeSelected();
    } else if (e.key === 'Escape') {
      if (app.preview) app.closePreview();
      else app.selected = null;
    } else if (e.key.toLowerCase() === 'r' && !ctrl) {
      app.rotateSelected(e.shiftKey ? -90 : 90);
    } else if (e.key.toLowerCase() === 'p' && !ctrl) {
      if (app.preview) app.closePreview();
      else if (app.plan && !app.planError) void app.openPreview();
    } else if (e.key === ' ' && app.preview) {
      e.preventDefault();
      app.preview.playing = !app.preview.playing;
    } else if (e.key === '0' && !ctrl) {
      app.fitRequest++;
    } else if (e.key === '+' || e.key === '=') {
      app.zoomBy(1.25);
    } else if (e.key === '-') {
      app.zoomBy(1 / 1.25);
    } else if (e.key.startsWith('Arrow') && app.selected !== null) {
      e.preventDefault();
      const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0;
      const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0;
      app.nudgeSelected(dx, dy);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <TopBar />
  <main class="workspace">
    <MatView />
    <PreviewBar />
  </main>
  <aside class="sidebar">
    <MachinePanel />
    <ColorList />
    <ItemPanel />
  </aside>
  <StatusBar />
</div>
<SendDialog />
<Toasts />

<style>
  .app {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 360px;
    grid-template-rows: auto minmax(0, 1fr) auto;
    height: 100%;
  }

  .app > :global(header) {
    grid-column: 1 / -1;
  }

  .app > :global(footer) {
    grid-column: 1 / -1;
  }

  .workspace {
    position: relative;
    display: grid;
    min-width: 0;
    min-height: 0;
  }

  .workspace > :global(.stage) {
    grid-area: 1 / 1;
  }

  .sidebar {
    overflow-y: auto;
    background: var(--panel);
    border-left: 1px solid var(--border);
  }
</style>
