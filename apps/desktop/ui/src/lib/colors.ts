// Friendly names for design colours: the nearest of a small palette, in the current language.
import { t } from './i18n/index.svelte';
import type { Key } from './i18n/en';

const PALETTE: [string, [number, number, number]][] = [
  ['black', [20, 20, 20]],
  ['white', [250, 250, 250]],
  ['grey', [128, 128, 128]],
  ['red', [220, 40, 40]],
  ['orange', [245, 140, 30]],
  ['yellow', [240, 210, 40]],
  ['green', [40, 170, 70]],
  ['teal', [20, 150, 140]],
  ['blue', [40, 100, 225]],
  ['navy', [25, 40, 110]],
  ['purple', [140, 60, 190]],
  ['pink', [235, 100, 170]],
  ['brown', [130, 80, 40]],
];

export function colorName(hex: string | null): string {
  if (!hex) return t('colors.noColor');
  if (hex.toLowerCase() === '#ff00ff') return t('colors.contour');
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!m) return hex;
  const rgb = [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
  let best = PALETTE[0];
  let bestD = Infinity;
  for (const entry of PALETTE) {
    // Weighted distance, close enough to how different colours look.
    const [r, g, b] = entry[1];
    const rm = (rgb[0] + r) / 2;
    const d = (2 + rm / 256) * (rgb[0] - r) ** 2 + 4 * (rgb[1] - g) ** 2 + (2 + (255 - rm) / 256) * (rgb[2] - b) ** 2;
    if (d < bestD) [best, bestD] = [entry, d];
  }
  return t(`color.${best[0]}` as Key);
}
