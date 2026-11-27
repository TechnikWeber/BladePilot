// Light or dark, following the system unless `?theme=` asks otherwise (for screenshots).
let dark = $state(false);

export function initTheme(): void {
  const forced = new URLSearchParams(location.search).get('theme');
  if (forced === 'dark' || forced === 'light') {
    document.documentElement.dataset.theme = forced;
    dark = forced === 'dark';
    return;
  }
  const query = matchMedia('(prefers-color-scheme: dark)');
  dark = query.matches;
  query.addEventListener('change', (e) => (dark = e.matches));
}

export function isDark(): boolean {
  return dark;
}

function parse(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : null;
}

function luminance([r, g, b]: [number, number, number]): number {
  const lin = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function mix([r, g, b]: [number, number, number], [r2, g2, b2]: [number, number, number], t: number): string {
  const h = (v: number) =>
    Math.round(v)
      .toString(16)
      .padStart(2, '0');
  return `#${h(r + (r2 - r) * t)}${h(g + (g2 - g) * t)}${h(b + (b2 - b) * t)}`;
}

/** A design colour as it is drawn on the mat: very dark colours lighten on a dark mat, very light ones darken on a light mat. */
export function displayColor(color: string | null): string {
  const rgb = color ? parse(color) : null;
  if (!rgb) return dark ? '#9aa7b6' : '#6b7685';
  const l = luminance(rgb);
  if (dark && l < 0.06) return mix(rgb, [226, 232, 240], 0.78);
  if (!dark && l > 0.75) return mix(rgb, [40, 50, 62], 0.55);
  return color as string;
}
