<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { app, deviceKey } from './lib/state.svelte';
  import { backend } from './lib/api';
  import type { DesignInfo, MadeSpec } from './lib/api/types';
  import { setLanguage } from './lib/i18n/index.svelte';
  import TopBar from './components/TopBar.svelte';
  import MatView from './components/MatView.svelte';
  import MachinePanel from './components/MachinePanel.svelte';
  import ColorList from './components/ColorList.svelte';
  import PrintPanel from './components/PrintPanel.svelte';
  import ItemPanel from './components/ItemPanel.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import PreviewBar from './components/PreviewBar.svelte';
  import SendDialog from './components/SendDialog.svelte';
  import Toasts from './components/Toasts.svelte';
  import UsbAccessDialog from './components/UsbAccessDialog.svelte';
  import MakeDialog from './components/MakeDialog.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';
  import UnsavedDialog from './components/UnsavedDialog.svelte';

  let ready = $state(false);

  onMount(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      // In development, VITE_STATE adds the same switches inside the desktop window.
      const extra = import.meta.env.DEV ? (import.meta.env.VITE_STATE ?? '') : '';
      const params = new URLSearchParams(location.search + (extra ? `&${extra}` : ''));
      await app.init();
      setLanguage(params.get('lang') ?? (app.languageChoice !== 'system' ? app.languageChoice : (app.info?.language ?? navigator.language)));
      ready = true;
      unlisten = await backend.onFileDrop(
        async (paths) => {
          for (const path of paths) await openPath(path);
        },
        (over) => (app.dropping = over),
      );
      if (!backend.mock) void app.scan();
      for (const path of await backend.startupFiles()) await openPath(path);
      await screenshotState(params);
    })();
    return () => unlisten?.();
  });

  /** A dropped or named file: a project replaces the mat, a design joins it. */
  async function openPath(path: string): Promise<void> {
    if (path.toLowerCase().endsWith('.bladepilot')) await app.openProjectPath(path);
    else await app.open(async () => [await backend.openPath(path)], path);
  }

  /** `?connected`, `?demo`, `?shapes`, `?htv`, `?multi`, `?settings`, `?align`, `?make=text|shape`, `?preview=0.6`, `?send[=change|start]`, `?test`, `?print`, `?select`: states for screenshots and tests. */
  async function screenshotState(params: URLSearchParams): Promise<void> {
    if (params.has('machine')) app.setMachine(params.get('machine') as string);
    if (params.has('connected')) {
      await app.scan();
      if (app.devices[0]) app.target = deviceKey(app.devices[0]);
    }
    if (params.has('demo')) {
      await app.openDemo();
      if (!params.has('select')) app.selected = null;
      else app.selected = app.items[0]?.id ?? null;
    }
    if (params.has('print')) {
      app.printMode = true;
      for (const item of app.items) item.cut = { kind: 'contour', offsetMm: 3 };
      app.selected = app.items[0]?.id ?? null;
      app.centerSelected();
    }
    if (params.has('shapes')) {
      // Vinyl: shapes with weeding boxes, arranged, the first one mirrored for heat transfer.
      const specs: MadeSpec[] = [
        { kind: 'shape', shape: 'heart', widthMm: 60, heightMm: 54, cornerMm: 0, sides: 5, inner: 0.5 },
        { kind: 'shape', shape: 'star', widthMm: 58, heightMm: 55, cornerMm: 0, sides: 5, inner: 0.45 },
        { kind: 'shape', shape: 'rectangle', widthMm: 80, heightMm: 30, cornerMm: 8, sides: 5, inner: 0.5 },
        { kind: 'shape', shape: 'polygon', widthMm: 40, heightMm: 40, cornerMm: 0, sides: 6, inner: 0.5 },
      ];
      const name: MadeSpec = { kind: 'text', text: 'Lena', family: '', sizeMm: 40, bold: true, italic: false, align: 'center', letterSpacingMm: 0, lineHeight: 1.2, weld: false };
      // The browser demo has no fonts: the text comes only in the desktop app.
      const made = await Promise.all([name, ...specs].map((spec) => backend.makeDesign(spec).catch(() => null)));
      await app.open(async () => made.filter((d): d is DesignInfo => d !== null));
      for (const item of app.items) item.weed = { marginMm: 4, lines: true };
      app.arrange();
      app.selected = app.items[0]?.id ?? null;
    }
    if (params.has('htv')) {
      // Heat transfer vinyl for everything: one pass, cut mirrored, the sliders in sight.
      const htv = app.materials.find((m) => m.id === 'b:htv');
      app.byColor = false;
      if (htv) app.applyMaterial(app.single, htv);
      app.toasts = [];
      await tick();
      document.querySelector('.single')?.scrollIntoView({ block: 'start' });
    }
    if (params.has('multi')) app.selectAll();
    if (params.has('settings')) app.settingsOpen = true;
    if (params.has('align')) await app.openAlignment();
    if (params.has('make')) {
      app.openMake(params.get('make') === 'text' ? 'text' : 'shape');
      if (app.make?.spec.kind === 'shape') app.make.spec = { ...app.make.spec, shape: 'star', widthMm: 60, heightMm: 57, inner: 0.45 };
    }
    if (!app.items.length) return;
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

  // The session is kept automatically, a moment after the last change, to pick up after a crash
  // or at the next start.
  $effect(() => {
    if (!ready || !app.items.length) return;
    const snapshot = JSON.stringify(app.projectRequest(true));
    const timer = setTimeout(() => void backend.autosave(JSON.parse(snapshot)).catch(() => {}), 1500);
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
    const ctrl = e.ctrlKey || e.metaKey;
    // Ctrl with + - 0 zooms the mat, never the page with its toolbar and panels.
    if (ctrl && ['+', '=', '-', '0'].includes(e.key)) {
      e.preventDefault();
      if (e.key === '0') app.fitRequest++;
      else app.zoomBy(e.key === '-' ? 1 / 1.25 : 1.25);
      return;
    }
    if (target.closest('input, select, textarea') || app.send || app.usbDialog || app.make || app.settingsOpen || app.unsavedAsk) return;
    const step = e.shiftKey ? 10 : 1;
    const key = e.key.toLowerCase();
    if (ctrl && key === 'n') {
      e.preventDefault();
      void app.newProject();
    } else if (ctrl && key === 's') {
      e.preventDefault();
      void app.saveProject(e.shiftKey);
    } else if (ctrl && key === 'a') {
      e.preventDefault();
      app.selectAll();
    } else if (ctrl && key === 'g') {
      e.preventDefault();
      if (e.shiftKey) app.ungroupSelected();
      else app.groupSelected();
    } else if (ctrl && e.key.toLowerCase() === 'o') {
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
    } else if (e.key.startsWith('Arrow') && app.selection.length) {
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
    <!-- The machine and its connection stay in sight; the rest scrolls below them. -->
    <MachinePanel />
    <div class="sidebar-scroll">
      <PrintPanel />
      <ColorList />
      <ItemPanel />
    </div>
  </aside>
  <StatusBar />
</div>
<SendDialog />
<UsbAccessDialog />
<MakeDialog />
<SettingsDialog />
<UnsavedDialog />
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
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--panel);
    border-left: 1px solid var(--border);
  }

  .sidebar > :global(section) {
    flex: none;
    box-shadow: 0 6px 10px -10px rgba(0, 0, 0, 0.35);
    z-index: 1;
  }

  .sidebar-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
</style>
