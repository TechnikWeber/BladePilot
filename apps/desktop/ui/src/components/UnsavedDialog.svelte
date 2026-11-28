<script lang="ts">
  // Before the mat is replaced (new, open): save the changes, throw them away, or stay.
  import { app } from '../lib/state.svelte';
  import { t } from '../lib/i18n/index.svelte';
  import Icon from './Icon.svelte';

  function answer(choice: 'save' | 'discard' | 'cancel'): void {
    app.unsavedAsk?.(choice);
  }
</script>

{#if app.unsavedAsk}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" role="presentation" onkeydown={(e) => e.key === 'Escape' && answer('cancel')}>
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="unsaved-title">
      <header>
        <div class="title-icon"><Icon name="save" size={18} /></div>
        <h2 id="unsaved-title">{app.projectName ? t('unsaved.titleNamed', { name: app.projectName }) : t('unsaved.title')}</h2>
      </header>
      <p>{t('unsaved.body')}</p>
      <footer>
        <button class="btn ghost" onclick={() => answer('discard')}>{t('unsaved.discard')}</button>
        <span class="spacer"></span>
        <button class="btn" onclick={() => answer('cancel')}>{t('unsaved.cancel')}</button>
        <button class="btn primary" onclick={() => answer('save')}><Icon name="save" size={14} />{t('unsaved.save')}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: rgba(10, 16, 24, 0.42);
  }

  .dialog {
    width: min(460px, calc(100vw - 32px));
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-3);
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 18px 20px 4px;
  }

  header h2 {
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

  p {
    margin: 8px 20px 4px 66px;
    color: var(--text-2);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 20px 18px;
  }

  .spacer {
    flex: 1;
  }
</style>
