import { describe, expect, it } from 'vitest';
import { type WheelLike, wheelAction, zoomFactor } from './navigation';

function wheel(over: Partial<WheelLike>): WheelLike {
  return { deltaX: 0, deltaY: 0, deltaMode: 0, ctrlKey: false, metaKey: false, shiftKey: false, ...over };
}

describe('navigation', () => {
  it('moves the view with two fingers on a touchpad, both ways at once', () => {
    expect(wheelAction(wheel({ deltaX: 3.5, deltaY: -7.25 }), 'pan')).toEqual({ kind: 'pan', dx: 3.5, dy: -7.25 });
  });

  it('zooms with Ctrl and the wheel, and more per step for a pinch', () => {
    expect(wheelAction(wheel({ deltaY: -4, ctrlKey: true }), 'pan')).toEqual({ kind: 'zoom', factor: zoomFactor(-4, true) });
    expect(wheelAction(wheel({ deltaY: 100, ctrlKey: true }), 'pan')).toEqual({ kind: 'zoom', factor: zoomFactor(100, false) });
    expect(zoomFactor(-4, true)).toBeGreaterThan(1);
    expect(zoomFactor(100, false)).toBeLessThan(1);
  });

  it('counts wheels that report lines or pages in pixels', () => {
    expect(wheelAction(wheel({ deltaY: 3, deltaMode: 1 }), 'pan')).toEqual({ kind: 'pan', dx: 0, dy: 48 });
    expect(wheelAction(wheel({ deltaY: 1, deltaMode: 2 }), 'pan')).toEqual({ kind: 'pan', dx: 0, dy: 600 });
  });

  it('turns a plain wheel sideways with Shift', () => {
    expect(wheelAction(wheel({ deltaY: 40, shiftKey: true }), 'pan')).toEqual({ kind: 'pan', dx: 40, dy: 0 });
    expect(wheelAction(wheel({ deltaY: 40, shiftKey: true }), 'zoom')).toEqual({ kind: 'pan', dx: 40, dy: 0 });
  });

  it('zooms with the plain wheel when the settings say so', () => {
    expect(wheelAction(wheel({ deltaY: 53 }), 'zoom')).toEqual({ kind: 'zoom', factor: zoomFactor(53, false) });
  });
});
