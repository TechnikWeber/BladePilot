<script lang="ts">
  // A setting with a known range: a slider with both ends written under it, and the value, which
  // can also be typed. With `optional`, the value can be left to the machine (null): the slider
  // then sits dimmed in the middle until it is moved.
  import { num, parseNum, t } from '../lib/i18n/index.svelte';
  import Icon from './Icon.svelte';

  let {
    label,
    value,
    min,
    max,
    step = 1,
    low = '',
    high = '',
    optional = false,
    disabled = false,
    emptyText = '',
    title = '',
    onchange,
  }: {
    label: string;
    value: number | null;
    min: number;
    max: number;
    step?: number;
    /** What the low end means, e.g. "light". */
    low?: string;
    high?: string;
    optional?: boolean;
    disabled?: boolean;
    /** Shown instead of a value when there is none. */
    emptyText?: string;
    title?: string;
    onchange: (v: number | null) => void;
  } = $props();

  let text = $state('');
  let editing = $state(false);

  const unset = $derived(value === null || value === undefined);
  const knob = $derived(unset ? Math.round((min + max) / 2 / step) * step : Math.min(max, Math.max(min, value as number)));
  const fill = $derived(((knob - min) / (max - min)) * 100);

  $effect(() => {
    if (!editing) text = unset ? '' : num(value as number, 0);
  });

  function clamp(v: number): number {
    return Math.min(max, Math.max(min, Math.round(v / step) * step));
  }

  function commit(): void {
    editing = false;
    const v = parseNum(text);
    if (v === null) {
      if (optional && text.trim() === '') onchange(null);
      else text = unset ? '' : num(value as number, 0);
      return;
    }
    onchange(clamp(v));
  }

  function key(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      commit();
      (e.target as HTMLInputElement).blur();
    } else if (e.key === 'Escape') {
      editing = false;
      text = unset ? '' : num(value as number, 0);
      (e.target as HTMLInputElement).blur();
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      const v = clamp((parseNum(text) ?? knob) + (e.key === 'ArrowUp' ? step : -step));
      text = num(v, 0);
      onchange(v);
    }
  }
</script>

<div class="sf" class:unset class:disabled {title}>
  <div class="sf-head">
    <span class="sf-label">{label}</span>
    <span class="spacer"></span>
    {#if optional && !unset && !disabled}
      <button class="sf-reset" title={t('settings.reset')} aria-label={t('settings.reset')} onclick={() => onchange(null)}><Icon name="undo" size={12} /></button>
    {/if}
    <input
      class="sf-value tnum"
      type="text"
      inputmode="numeric"
      aria-label={label}
      {disabled}
      placeholder={emptyText}
      bind:value={text}
      onfocus={() => (editing = true)}
      onblur={commit}
      onkeydown={key}
    />
  </div>
  <input
    class="sf-range"
    type="range"
    aria-label={label}
    {min}
    {max}
    {step}
    {disabled}
    value={knob}
    style="--fill: {fill}%"
    oninput={(e) => onchange(clamp(Number((e.target as HTMLInputElement).value)))}
  />
  <div class="sf-ends tnum">
    <span>{num(min, 0)}{low ? ` · ${low}` : ''}</span>
    <span>{high ? `${high} · ` : ''}{num(max, 0)}</span>
  </div>
</div>

<style>
  .sf {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .sf-head {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .sf-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }

  .spacer {
    flex: 1;
  }

  .sf-value {
    width: 64px;
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel-2);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    font-weight: 650;
    text-align: right;
  }

  .sf-value::placeholder {
    color: var(--text-3);
    font-weight: 500;
  }

  .sf-value:focus {
    outline: 2px solid var(--accent-line);
    outline-offset: -1px;
  }

  .sf-reset {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--text-3);
    cursor: pointer;
  }

  .sf-reset:hover {
    background: var(--hover);
    color: var(--text);
  }

  .sf-range {
    width: 100%;
    height: 20px;
    margin: 0;
    background: transparent;
    accent-color: var(--accent);
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
  }

  .sf-range::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) var(--fill), var(--border) var(--fill));
  }

  .sf-range::-moz-range-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) var(--fill), var(--border) var(--fill));
  }

  .sf-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -5px;
    border: 2px solid var(--panel);
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 1px var(--accent-line);
  }

  .sf-range::-moz-range-thumb {
    width: 10px;
    height: 10px;
    border: 2px solid var(--panel);
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 1px var(--accent-line);
  }

  .unset .sf-range::-webkit-slider-runnable-track {
    background: var(--border);
  }

  .unset .sf-range::-moz-range-track {
    background: var(--border);
  }

  .unset .sf-range::-webkit-slider-thumb {
    background: var(--text-3);
    box-shadow: none;
  }

  .unset .sf-range::-moz-range-thumb {
    background: var(--text-3);
    box-shadow: none;
  }

  .sf-ends {
    display: flex;
    justify-content: space-between;
    color: var(--text-3);
    font-size: 11px;
  }

  .disabled {
    opacity: 0.5;
  }

  .disabled .sf-range {
    cursor: default;
  }
</style>
