// Material profiles: settings to start from for common media, and the user's own. The built-in
// values are BladePilot's own rough starting points for a fresh blade on a Portrait, not taken
// from any vendor table; the test cut tells whether they fit the roll at hand.
import type { Settings, Tool } from './api/types';

export interface Material {
  /** `b:` built in, `u:` the user's own. */
  id: string;
  /** Built in: the i18n key of its name; the user's own: the name itself. */
  name: string;
  builtin: boolean;
  settings: Settings;
  /** Heat transfer vinyl is cut from the back: the job is mirrored. */
  mirror?: boolean;
}

function s(tool: Tool, force: number, speed: number, depth: number | null, repeats = 1): Settings {
  return { tool, force, speed, depth, repeats, overcutMm: 0, bladeOffsetMm: 0.9 };
}

export const BUILTIN_MATERIALS: Material[] = [
  { id: 'b:copyPaper', name: 'material.copyPaper', builtin: true, settings: s('blade', 10, 5, 2) },
  { id: 'b:cardstock', name: 'material.cardstock', builtin: true, settings: s('blade', 25, 3, 5, 2) },
  { id: 'b:stickerPaper', name: 'material.stickerPaper', builtin: true, settings: s('blade', 14, 5, 2) },
  { id: 'b:vinyl', name: 'material.vinyl', builtin: true, settings: s('blade', 8, 5, 1) },
  { id: 'b:htv', name: 'material.htv', builtin: true, settings: s('blade', 12, 5, 2), mirror: true },
  { id: 'b:glitterHtv', name: 'material.glitterHtv', builtin: true, settings: s('blade', 22, 3, 3), mirror: true },
  { id: 'b:pen', name: 'material.pen', builtin: true, settings: s('pen', 10, 5, null) },
];

const KEY = 'bladepilot.materials.v1';

export function loadMaterials(): Material[] {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) ?? '[]') as Material[];
    return Array.isArray(list) ? list.filter((m) => typeof m.id === 'string' && m.settings).map((m) => ({ ...m, builtin: false })) : [];
  } catch {
    return [];
  }
}

export function storeMaterials(list: Material[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Nothing to keep them in: they last this session.
  }
}

/** The profile these settings came from, when they still match one. */
export function matchMaterial(settings: Settings, list: Material[]): Material | null {
  const same = (a: Settings, b: Settings) =>
    a.tool === b.tool && a.force === b.force && a.speed === b.speed && (a.tool === 'pen' || a.depth === b.depth) && a.repeats === b.repeats;
  return list.find((m) => same(m.settings, settings)) ?? null;
}
