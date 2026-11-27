<script lang="ts">
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">
      <Icon name={toast.kind === 'error' ? 'alert' : 'info'} size={16} />
      <span>{toast.text}</span>
      <button class="icon-btn sm" aria-label="Close" onclick={() => (app.toasts = app.toasts.filter((x) => x.id !== toast.id))}><Icon name="close" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 376px;
    bottom: 44px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 50;
    max-width: 420px;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 8px 10px 12px;
    border-radius: 10px;
    background: var(--panel);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-2);
    font-size: 12.5px;
    animation: up 0.18s ease-out;
  }

  .toast span {
    flex: 1;
    padding-top: 1px;
  }

  .toast.error :global(.icon:first-child) {
    color: var(--danger);
    margin-top: 1px;
  }

  .toast.info :global(.icon:first-child) {
    color: var(--accent);
    margin-top: 1px;
  }

  @keyframes up {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
  }
</style>
