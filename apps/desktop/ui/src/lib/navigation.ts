// How the view follows the wheel and the touchpad. Scrolling moves the view, as in Inkscape and on
// every touchpad (two fingers); Ctrl with the wheel or a pinch zooms. Who prefers the wheel to
// zoom switches the mode to 'zoom' in the settings; Shift then still moves sideways.

export type ScrollMode = 'pan' | 'zoom';

export type WheelAction = { kind: 'zoom'; factor: number } | { kind: 'pan'; dx: number; dy: number };

/** The parts of a WheelEvent that matter here. */
export interface WheelLike {
  deltaX: number;
  deltaY: number;
  deltaMode: number;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

/** Pixels per line and per page, for wheels that count in lines or pages (Firefox). */
const LINE_PX = 16;
const PAGE_PX = 600;

/** How much a zoom step zooms: a pinch (small steps with Ctrl) more per pixel than a wheel. */
export function zoomFactor(delta: number, pinch: boolean): number {
  return Math.exp(-delta * (pinch ? 0.01 : 0.0018));
}

/** What a wheel event over the mat does: zoom by a factor, or move the view like a page scrolls. */
export function wheelAction(e: WheelLike, mode: ScrollMode): WheelAction {
  const k = e.deltaMode === 1 ? LINE_PX : e.deltaMode === 2 ? PAGE_PX : 1;
  let dx = e.deltaX * k;
  let dy = e.deltaY * k;
  if (e.ctrlKey || e.metaKey) return { kind: 'zoom', factor: zoomFactor(dy, Math.abs(dy) < 50) };
  // Shift turns a plain wheel sideways.
  if (e.shiftKey && dx === 0) [dx, dy] = [dy, 0];
  if (mode === 'zoom' && !e.shiftKey) return { kind: 'zoom', factor: zoomFactor(dy, false) };
  return { kind: 'pan', dx, dy };
}
