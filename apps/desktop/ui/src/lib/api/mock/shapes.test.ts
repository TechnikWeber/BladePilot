import { describe, expect, it } from 'vitest';
import { madeDesign } from './shapes';
import machinesJson from './machines.json';
import { planMock } from './planner';
import type { MachineInfo, ShapeName } from '../types';

describe('the mock shapes', () => {
  it('fill the box they are asked for', () => {
    for (const shape of ['rectangle', 'ellipse', 'polygon', 'star', 'heart'] as ShapeName[]) {
      const d = madeDesign({ kind: 'shape', shape, widthMm: 40, heightMm: 20, cornerMm: 4, sides: 6, inner: 0.5 });
      expect(d.bbox[0]).toBeCloseTo(0, 1);
      expect(d.bbox[2]).toBeCloseTo(40, 0);
      expect(d.bbox[3]).toBeCloseTo(20, 0);
      expect(d.made?.kind).toBe('shape');
    }
  });

  it('cut a weeding box last, in a pass of its own', () => {
    const design = { ...madeDesign({ kind: 'shape', shape: 'star', widthMm: 50, heightMm: 50, cornerMm: 0, sides: 5, inner: 0.5 }), id: 1 };
    const blade = { tool: 'autoBlade' as const, force: null, speed: null, depth: null, repeats: 1, overcutMm: 0, bladeOffsetMm: 0.9 };
    const { plan } = planMock(
      {
        machine: 'portrait3',
        mat: '8x12',
        items: [{ design: 1, matrix: [1, 0, 0, 1, 20, 20], cut: { kind: 'lines', outerOnly: false, hidden: [] }, print: false, weed: { marginMm: 5, lines: false } }],
        print: null,
        byColor: true,
        manualOrder: false,
        settings: blade,
        colors: [],
      },
      new Map([[1, design]]),
      machinesJson as MachineInfo[],
    );
    expect(plan.passes.map((p) => p.color)).toEqual(['#000000', '#00a0a0']);
    expect(plan.bbox?.[0]).toBeCloseTo(15, 1);
  });

  it('leave text to the desktop app', () => {
    expect(() => madeDesign({ kind: 'text', text: 'Hi', family: '', sizeMm: 20, bold: false, italic: false, align: 'left', letterSpacingMm: 0, lineHeight: 1.2, weld: false })).toThrow();
  });
});
