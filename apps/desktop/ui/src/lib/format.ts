// Lengths in the unit the user chose.
import { app } from './state.svelte';
import { num } from './i18n/index.svelte';
import { MM_PER_INCH } from './geometry';

export function fmtLength(mm: number): string {
  if (app.unit === 'in') return `${num(mm / MM_PER_INCH, mm < 254 ? 2 : 1)} in`;
  if (mm >= 1000) return `${num(mm / 1000, 2)} m`;
  return `${num(mm, mm < 100 ? 1 : 0)} mm`;
}

/** A length as a number in the chosen unit, for input fields. */
export function toUnit(mm: number): number {
  return app.unit === 'in' ? mm / MM_PER_INCH : mm;
}

export function fromUnit(value: number): number {
  return app.unit === 'in' ? value * MM_PER_INCH : value;
}

export function unitLabel(): string {
  return app.unit === 'in' ? 'in' : 'mm';
}
