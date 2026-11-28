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
  /** For the pen (drawing) rather than a blade (cutting). */
  pen: boolean;
  settings: Settings;
  /** Heat transfer vinyl is cut from the back: the job is mirrored. */
  mirror?: boolean;
}

function s(tool: Tool, force: number, speed: number, depth: number | null, repeats = 1): Settings {
  return { tool, force, speed, depth, repeats, overcutMm: 0, bladeOffsetMm: 0.9 };
}

export const BUILTIN_MATERIALS: Material[] = [
  { id: 'b:copyPaper', name: 'material.copyPaper', builtin: true, pen: false, settings: s('autoBlade', 10, 5, 2) },
  { id: 'b:cardstock', name: 'material.cardstock', builtin: true, pen: false, settings: s('autoBlade', 25, 3, 5, 2) },
  { id: 'b:stickerPaper', name: 'material.stickerPaper', builtin: true, pen: false, settings: s('autoBlade', 14, 5, 2) },
  // Vinyl with a printable top coat on its backing: a little thicker than plain vinyl.
  { id: 'b:printableVinyl', name: 'material.printableVinyl', builtin: true, pen: false, settings: s('autoBlade', 12, 4, 2) },
  { id: 'b:vinyl', name: 'material.vinyl', builtin: true, pen: false, settings: s('autoBlade', 8, 5, 1) },
  { id: 'b:htv', name: 'material.htv', builtin: true, pen: false, settings: s('autoBlade', 12, 5, 2), mirror: true },
  { id: 'b:glitterHtv', name: 'material.glitterHtv', builtin: true, pen: false, settings: s('autoBlade', 22, 3, 3), mirror: true },
  // Flock is soft and thick: more force, slower, a deeper blade.
  { id: 'b:flockHtv', name: 'material.flockHtv', builtin: true, pen: false, settings: s('autoBlade', 18, 3, 3), mirror: true },
  { id: 'b:penPaper', name: 'material.penPaper', builtin: true, pen: true, settings: s('pen', 10, 5, null) },
  { id: 'b:penCard', name: 'material.penCard', builtin: true, pen: true, settings: s('pen', 14, 4, null) },
];

const KEY = 'bladepilot.materials.v1';

/** Files and preferences from before 0.8 call the AutoBlade `blade`. */
export function normalizeTool(tool: string | undefined): Tool {
  const known: Tool[] = ['autoBlade', 'ratchet', 'deepCut', 'kraft', 'rotary', 'pen'];
  return known.includes(tool as Tool) ? (tool as Tool) : 'autoBlade';
}

export function loadMaterials(): Material[] {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) ?? '[]') as Material[];
    if (!Array.isArray(list)) return [];
    return list
      .filter((m) => typeof m.id === 'string' && m.settings)
      .map((m) => {
        const tool = normalizeTool(m.settings.tool);
        return { ...m, builtin: false, pen: tool === 'pen', settings: { ...m.settings, tool } };
      });
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

/** The materials that go with a tool: the pen's for the pen, the blades' for any blade. */
export function materialsFor(tool: Tool, list: Material[]): Material[] {
  return list.filter((m) => m.pen === (tool === 'pen'));
}

/** The profile these settings came from, when they still match one (any blade counts as the same). */
export function matchMaterial(settings: Settings, list: Material[]): Material | null {
  const same = (a: Settings, b: Settings) =>
    (a.tool === 'pen') === (b.tool === 'pen') && a.force === b.force && a.speed === b.speed && (a.tool === 'pen' || a.depth === b.depth) && a.repeats === b.repeats;
  return list.find((m) => same(m.settings, settings)) ?? null;
}

/** The tool the machine reports in its holder, as a pass's tool; `null` when it says nothing useful. */
export function reportedTool(code: string | undefined): Tool | null {
  switch (code) {
    case 'autoBlade':
      return 'autoBlade';
    case 'ratchetBlade':
      return 'ratchet';
    case 'deepCutBlade':
      return 'deepCut';
    case 'kraftBlade':
      return 'kraft';
    case 'rotaryBlade':
      return 'rotary';
    case 'pen':
      return 'pen';
    default:
      return null;
  }
}
