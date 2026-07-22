import { describe, expect, it } from 'vitest';
import {
  adjustRowSplitRatios,
  clampSplitRatio,
  computeTileLayout,
  computeTileStyles,
  distributeRows,
  HISTORY_PURGE_KEY,
  loadWorkspaces,
  moveTerminal,
  normalizeSplitRatiosForRows,
  purgeCapturedCommandHistory,
  STORAGE_KEY,
} from './workspaces';
import type { Workspace } from './types';

function fakeStorage(entries: Record<string, string> = {}) {
  const data = new Map(Object.entries(entries));
  return {
    data,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
  };
}

describe('automatic terminal layout', () => {
  it.each([
    [1, [1]], [2, [2]], [3, [2, 1]], [4, [2, 2]],
    [5, [3, 2]], [6, [3, 3]], [7, [3, 2, 2]], [9, [3, 3, 3]],
  ])('fills the stage for %i terminals', (count, expected) => {
    const items = Array.from({ length: count }, (_, index) => ({ id: String(index), name: '', cwd: '' }));
    expect(distributeRows(items).map((row) => row.length)).toEqual(expected);
    expect(Object.keys(computeTileStyles(items))).toHaveLength(count);
  });

  it('uses custom row split ratios for four terminals across two rows', () => {
    const terminals = [
      { id: 'one', name: 'One', cwd: '/a' },
      { id: 'two', name: 'Two', cwd: '/a' },
      { id: 'three', name: 'Three', cwd: '/a' },
      { id: 'four', name: 'Four', cwd: '/a' },
    ];
    const styles = computeTileStyles(terminals, [
      [0.6, 0.4],
      [0.35, 0.65],
    ]);
    expect(styles.one).toContain('width: calc(60% - 8px)');
    expect(styles.two).toContain('left: calc(60% + 4px)');
    expect(styles.three).toContain('width: calc(35% - 8px)');
    expect(styles.four).toContain('left: calc(35% + 4px)');
  });

  it('builds one resize handle per side-by-side boundary', () => {
    const terminals = Array.from({ length: 4 }, (_, index) => ({ id: String(index), name: '', cwd: '' }));
    const layout = computeTileLayout(terminals, [[0.5, 0.5], [0.3, 0.7]]);
    expect(layout.handles).toEqual([
      { rowIndex: 0, handleIndex: 0, leftPercent: 50, topPercent: 0, heightPercent: 50 },
      { rowIndex: 1, handleIndex: 0, leftPercent: 30, topPercent: 50, heightPercent: 50 },
    ]);
  });

  it('normalizes invalid stored split ratios per row', () => {
    const rows = [
      [{ id: 'a' }, { id: 'b' }],
      [{ id: 'c' }, { id: 'd' }, { id: 'e' }],
    ];
    const normalized = normalizeSplitRatiosForRows(rows, [[2, 1], ['bad']]);
    expect(normalized[0][0]).toBeCloseTo(2 / 3);
    expect(normalized[0][1]).toBeCloseTo(1 / 3);
    expect(normalized[1]).toEqual([1 / 3, 1 / 3, 1 / 3]);
  });

  it('clamps split ratio values to safe limits', () => {
    expect(clampSplitRatio(0.01)).toBe(0.2);
    expect(clampSplitRatio(0.99)).toBe(0.8);
  });

  it('adjusts a specific row boundary while preserving row totals', () => {
    const next = adjustRowSplitRatios(
      [[0.5, 0.5], [0.3, 0.7]],
      1,
      0,
      0.4,
    );
    expect(next).toEqual([[0.5, 0.5], [0.4, 0.6]]);
  });

  it('clamps row boundary adjustments to protect minimum pane sizes', () => {
    const next = adjustRowSplitRatios(
      [[0.5, 0.5]],
      0,
      0,
      0.01,
      0.1,
    );
    expect(next).toEqual([[0.1, 0.9]]);
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

  describe('retained terminal state', () => {
    it('restores persisted commands and scrollback for the matching terminal', () => {
      const storage = {
        getItem: () => JSON.stringify([{
          id: 'workspace',
          name: 'Workspace',
          cwd: '/home/dennis',
          activeTerminalId: 'terminal',
          terminals: [{
            id: 'terminal',
            name: 'Terminal',
            cwd: '/home/dennis/project',
            commandHistory: ['pwd', 'npm test'],
            scrollback: ['PS> pwd', '/home/dennis/project'],
            scrollbackAnsi: '\u001b[36mPS>\u001b[0m pwd',
          }],
        }]),
      };

      const terminal = loadWorkspaces(storage)[0].terminals[0];
      expect(terminal.commandHistory).toEqual(['pwd', 'npm test']);
      expect(terminal.scrollback).toEqual(['PS> pwd', '/home/dennis/project']);
      expect(terminal.scrollbackAnsi).toBe('\u001b[36mPS>\u001b[0m pwd');
    });
  });
});

describe('purging history captured by keystroke sniffing', () => {
  const stored = (history: string[]) => JSON.stringify([{
    id: 'workspace_1',
    name: 'Workspace',
    cwd: '/home/dennis',
    terminals: [{ id: 'term_1', name: 'Terminal 1', cwd: '/home/dennis', commandHistory: history }],
    activeTerminalId: 'term_1',
  }]);

  it('drops history a previous build may have captured from a password prompt', () => {
    const storage = fakeStorage({ [STORAGE_KEY]: stored(['ssh dennis@host', 'hunter2']) });

    expect(purgeCapturedCommandHistory(storage)).toBe(true);

    expect(storage.getItem(STORAGE_KEY)).not.toContain('hunter2');
    expect(loadWorkspaces(storage)[0].terminals[0].commandHistory).toEqual([]);
    expect(storage.getItem(HISTORY_PURGE_KEY)).toBe('1');
  });

  it('preserves everything except the command history', () => {
    const storage = fakeStorage({ [STORAGE_KEY]: stored(['secret']) });

    purgeCapturedCommandHistory(storage);

    const workspace = loadWorkspaces(storage)[0];
    expect(workspace.name).toBe('Workspace');
    expect(workspace.terminals[0].cwd).toBe('/home/dennis');
  });

  it('only purges once so history recorded afterwards survives', () => {
    const storage = fakeStorage({ [STORAGE_KEY]: stored(['hunter2']) });
    purgeCapturedCommandHistory(storage);

    storage.setItem(STORAGE_KEY, stored(['ls -la']));
    expect(purgeCapturedCommandHistory(storage)).toBe(false);

    expect(loadWorkspaces(storage)[0].terminals[0].commandHistory).toEqual(['ls -la']);
  });

  it('marks itself done even when nothing is stored yet', () => {
    const storage = fakeStorage();
    expect(purgeCapturedCommandHistory(storage)).toBe(true);
    expect(storage.getItem(HISTORY_PURGE_KEY)).toBe('1');
  });
});
