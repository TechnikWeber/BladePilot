// English and German, chosen from the system language. `t()` reads reactive state, so every text
// in a template follows a change of language.
import de from './de';
import en, { type Key } from './en';
import type { ApiError } from '../api/types';

export type Language = 'en' | 'de';

let current = $state<Language>('en');

export function setLanguage(tag: string): void {
  current = tag.toLowerCase().startsWith('de') ? 'de' : 'en';
  if (typeof document !== 'undefined') document.documentElement.lang = current;
}

export function language(): Language {
  return current;
}

export function t(key: Key, params?: Record<string, string | number>): string {
  let text: string = (current === 'de' ? de : en)[key] ?? en[key] ?? key;
  if (params) for (const [name, value] of Object.entries(params)) text = text.replaceAll(`{${name}}`, String(value));
  return text;
}

/** A number in the current language: `12.5` or `12,5`. */
export function num(value: number, digits = 1): string {
  return new Intl.NumberFormat(current === 'de' ? 'de-DE' : 'en-US', { maximumFractionDigits: digits, minimumFractionDigits: 0 }).format(value);
}

/** Reads a number typed in either language (`12,5` or `12.5`); `null` when it is none. */
export function parseNum(text: string): number | null {
  const cleaned = text.trim().replace(/\s/g, '').replace(',', '.');
  if (cleaned === '' || !/^[-+]?\d*\.?\d+$|^[-+]?\d+\.$/.test(cleaned)) return null;
  const value = Number(cleaned);
  return Number.isFinite(value) ? value : null;
}

/** The text for an error from the Rust side, translated when its code is known. */
export function errorText(error: ApiError | unknown): string {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const e = error as ApiError;
    const key = `error.${e.code}` as Key;
    if (key in en) return t(key);
    return t('error.generic', { message: e.message });
  }
  return t('error.generic', { message: String(error) });
}
