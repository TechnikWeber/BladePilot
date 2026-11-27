import { describe, expect, it } from 'vitest';
import demo from './api/mock/demo.json';
import type { DesignInfo } from './api/types';
import { apply, convexHull, flattenPathData, invert, multiply, placedBox, placementMatrix, polylineLength, rotate, snap1d } from './geometry';

const design = demo as unknown as DesignInfo;

describe('geometry', () => {
  it('a placement without turning or scaling only moves', () => {
    const m = placementMatrix({ cx: 100, cy: 80, angle: 0, sx: 1, sy: 1 }, design);
    const [x0, y0, x1, y1] = design.bbox;
    const tl = apply(m, { x: x0, y: y0 });
    expect(tl.x).toBeCloseTo(100 - (x1 - x0) / 2, 9);
    expect(tl.y).toBeCloseTo(80 - (y1 - y0) / 2, 9);
  });

  it('a quarter turn swaps width and height of the box', () => {
    const [x0, y0, x1, y1] = design.bbox;
    const box = placedBox({ cx: 100, cy: 100, angle: 90, sx: 1, sy: 1 }, design);
    expect(box.x1 - box.x0).toBeCloseTo(y1 - y0, 6);
    expect(box.y1 - box.y0).toBeCloseTo(x1 - x0, 6);
  });

  it('matrices invert and compose', () => {
    const m = multiply(rotate(30), [2, 0, 0, 3, 5, 7]);
    const p = apply(invert(m), apply(m, { x: 3, y: 4 }));
    expect(p.x).toBeCloseTo(3, 9);
    expect(p.y).toBeCloseTo(4, 9);
  });

  it('path data flattens with closed subpaths', () => {
    const [square] = flattenPathData('M0,0L10,0L10,10L0,10Z');
    expect(square).toEqual([0, 0, 10, 0, 10, 10, 0, 10, 0, 0]);
    const [circle] = flattenPathData(design.shapes[1].d, 0.2);
    expect(polylineLength(circle)).toBeCloseTo(2 * Math.PI * 3.5, 0);
  });

  it('snapping picks the nearest target within reach', () => {
    const hit = snap1d([10.4, 30], [0, 10, 20], 1);
    expect(hit?.target).toBe(10);
    expect(hit?.shift).toBeCloseTo(-0.4, 9);
    expect(snap1d([15], [0, 10, 20], 1)).toBeNull();
  });

  it('the hull leaves inner points out', () => {
    expect(convexHull([[0, 0], [4, 0], [4, 4], [0, 4], [2, 2]])).toHaveLength(4);
  });
});
