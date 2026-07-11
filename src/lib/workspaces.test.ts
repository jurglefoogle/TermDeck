import { describe, expect, it } from 'vitest';
import { computeTileStyles, distributeRows, moveTerminal } from './workspaces';
import type { Workspace } from './types';

describe('automatic terminal layout', () => {
  it.each([
    [1, [1]], [2, [2]], [3, [2, 1]], [4, [2, 2]],
    [5, [3, 2]], [6, [3, 3]], [7, [3, 2, 2]], [9, [3, 3, 3]],
  ])('fills the stage for %i terminals', (count, expected) => {
    const items = Array.from({ length: count }, (_, index) => ({ id: String(index), name: '', cwd: '' }));
    expect(distributeRows(items).map((row) => row.length)).toEqual(expected);
    expect(Object.keys(computeTileStyles(items))).toHaveLength(count);
  });
});

describe('workspace terminal movement', () => {
  it('moves a live terminal config and selects sensible neighbors', () => {
    const workspaces: Workspace[] = [
      { id: 'a', name: 'A', cwd: '/a', terminals: [
        { id: 'one', name: 'One', cwd: '/a' },
        { id: 'two', name: 'Two', cwd: '/a' },
      ], activeTerminalId: 'one' },
      { id: 'b', name: 'B', cwd: '/b', terminals: [], activeTerminalId: null },
    ];
    const moved = moveTerminal(workspaces, 'one', 'a', 'b');
    expect(moved[0].terminals.map((terminal) => terminal.id)).toEqual(['two']);
    expect(moved[0].activeTerminalId).toBe('two');
    expect(moved[1].terminals.map((terminal) => terminal.id)).toEqual(['one']);
    expect(moved[1].activeTerminalId).toBe('one');
  });
});

