<script lang="ts">
  // A number input that reads both decimal comma and point, commits on Enter or blur, and steps
  // with the arrow keys. Empty means "not set" when `optional` is on.
  import { num, parseNum } from '../lib/i18n/index.svelte';

  let {
    value,
    onchange,
    min = -Infinity,
    max = Infinity,
    step = 1,
    digits = 1,
    unit = '',
    optional = false,
    placeholder = '',
    disabled = false,
    label = '',
    title = '',
  }: {
    value: number | null;
    onchange: (v: number | null) => void;
    min?: number;
    max?: number;
    step?: number;
    digits?: number;
    unit?: string;
    optional?: boolean;
    placeholder?: string;
    disabled?: boolean;
    label?: string;
    title?: string;
  } = $props();

  let text = $state('');
  let editing = $state(false);

  $effect(() => {
    if (!editing) text = value === null || value === undefined ? '' : num(value, digits);
  });

  function commit(): void {
    editing = false;
    const v = parseNum(text);
    if (v === null) {
      if (optional && text.trim() === '') onchange(null);
      else text = value === null ? '' : num(value, digits);
      return;
    }
    onchange(Math.min(max, Math.max(min, v)));
  }

  function key(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      commit();
      (e.target as HTMLInputElement).blur();
    } else if (e.key === 'Escape') {
      editing = false;
      text = value === null ? '' : num(value, digits);
      (e.target as HTMLInputElement).blur();
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      const base = parseNum(text) ?? value ?? (min > -Infinity ? min : 0);
      const delta = (e.key === 'ArrowUp' ? 1 : -1) * step * (e.shiftKey ? 10 : 1);
      const v = Math.min(max, Math.max(min, Math.round((base + delta) / step) * step));
      text = num(v, digits);
      onchange(v);
    }
  }
</script>

<label class="nf" class:disabled {title}>
  {#if label}<span class="nf-label">{label}</span>{/if}
  <span class="nf-box">
    <input
      class="input tnum"
      type="text"
      inputmode="decimal"
      {disabled}
      {placeholder}
      bind:value={text}
      onfocus={() => (editing = true)}
      onblur={commit}
      onkeydown={key}
    />
    {#if unit}<span class="nf-unit">{unit}</span>{/if}
  </span>
</label>

<style>
  .nf {
    display: block;
    min-width: 0;
  }

  .nf-label {
    display: block;
    margin-bottom: 4px;
    color: var(--text-2);
    font-size: 12px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .nf-box {
    position: relative;
    display: block;
  }

  .nf-box .input {
    padding-right: 30px;
  }

  .nf-unit {
    position: absolute;
    right: 9px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-3);
    font-size: 12px;
    pointer-events: none;
  }

  .disabled {
    opacity: 0.5;
  }
</style>
