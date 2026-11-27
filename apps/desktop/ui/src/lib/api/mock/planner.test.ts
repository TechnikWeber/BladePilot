import { describe, expect, it } from 'vitest';
import demo from './demo.json';
import machinesJson from './machines.json';
import { planMock, simulateMock } from './planner';
import type { DesignInfo, JobRequest, MachineInfo, Settings } from '../types';

const design = { ...(demo as unknown as DesignInfo), id: 1 };
const machines = machinesJson as MachineInfo[];
const designs = new Map([[1, design]]);
const blade: Settings = { tool: 'blade', force: null, speed: null, depth: null, repeats: 1, overcutMm: 0, bladeOffsetMm: 0.9 };
const RED = '#e5484d';
const BLACK = '#1f2328';
const BLUE = '#2f6fde';

function job(edit: (j: JobRequest) => void = () => {}): JobRequest {
  const j: JobRequest = {
    machine: 'portrait3',
    mat: '8x12',
    items: [{ design: 1, matrix: [1, 0, 0, 1, 0, 0], cut: { kind: 'lines', outerOnly: false, hidden: [] }, print: false }],
    print: null,
    byColor: true,
    manualOrder: false,
    settings: blade,
    colors: [
      { color: BLACK, settings: blade, skip: false, pauseBefore: false },
      { color: RED, settings: { ...blade, tool: 'pen' }, skip: false, pauseBefore: false },
      { color: BLUE, settings: blade, skip: false, pauseBefore: false },
    ],
  };
  edit(j);
  return j;
}

describe('the mock planner', () => {
  it('puts the pen first and waits for the blade', () => {
    const { plan } = planMock(job(), designs, machines);
    expect(plan.passes.map((p) => p.color)).toEqual([RED, BLACK, BLUE]);
    expect(plan.passes[1].toolChange).toBe(true);
    expect(plan.passes[1].pauseBefore).toBe(true);
    expect(plan.conflicts).toEqual([]);
  });

  it('keeps a manual order and names what it cuts loose too early', () => {
    const { plan } = planMock(job((j) => { j.manualOrder = true; j.colors = [j.colors[2], j.colors[1], j.colors[0]]; }), designs, machines);
    expect(plan.passes.map((p) => p.color)).toEqual([BLUE, RED, BLACK]);
    expect(plan.conflicts).toContainEqual([0, 1]);
  });

  it('leaves skipped colours out and refuses an empty job', () => {
    expect(planMock(job((j) => (j.colors[1].skip = true)), designs, machines).plan.passes).toHaveLength(2);
    expect(() => planMock(job((j) => j.colors.forEach((c) => (c.skip = true))), designs, machines)).toThrowError();
  });

  it('refuses a design off the mat', () => {
    try {
      planMock(job((j) => (j.items[0].matrix = [1, 0, 0, 1, 100, 0])), designs, machines);
      expect.unreachable();
    } catch (e) {
      expect((e as { code: string }).code).toBe('outOfBounds');
    }
  });

  it('simulates one cut per path and repeat, each with its pass', () => {
    const sim = simulateMock(job((j) => (j.colors[0].settings = { ...blade, repeats: 2 })), designs, machines);
    const cuts = sim.strokes.filter((s) => s.cut);
    const expected = sim.plan.passes.reduce((n, p) => n + p.paths * p.settings.repeats, 0);
    expect(cuts).toHaveLength(expected);
    expect(cuts.every((s) => s.pass !== null)).toBe(true);
    expect(sim.strokes[sim.strokes.length - 1].cut).toBe(false);
  });
});
