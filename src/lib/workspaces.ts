import type { TerminalSession, Workspace } from './types';

export const STORAGE_KEY = 'termdeck.workspaces.v2';
export const LEGACY_STORAGE_KEY = 'termdeck.workspaces.v1';
export const ACTIVE_WORKSPACE_KEY = 'termdeck.active-workspace.v2';
export const MIN_SPLIT_RATIO = 0.2;
export const MAX_SPLIT_RATIO = 0.8;
export const DEFAULT_SPLIT_RATIO = 0.5;
export const DEFAULT_ROW_MIN_SPLIT = 0.1;

export function clampSplitRatio(value: number): number {
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, value));
}

export type TileHandle = {
  rowIndex: number;
  handleIndex: number;
  leftPercent: number;
  topPercent: number;
  heightPercent: number;
};

export type TileLayout = {
  styles: Record<string, string>;
  handles: TileHandle[];
  rowSplitRatios: number[][];
};

export function makeId(prefix: string): string {
  const value = typeof crypto?.randomUUID === 'function'
    ? crypto.randomUUID().replaceAll('-', '')
    : `${Date.now()}${Math.random().toString(16).slice(2)}`;
  return `${prefix}_${value}`;
}

export function createTerminal(index: number, cwd: string, name?: string): TerminalSession {
  return {
    id: makeId('term'),
    name: name?.trim() || `Terminal ${index}`,
    cwd,
  };
}

export function createWorkspace(name: string, cwd: string): Workspace {
  const terminal = createTerminal(1, cwd);
  return {
    id: makeId('workspace'),
    name,
    cwd,
    terminals: [terminal],
    activeTerminalId: terminal.id,
    splitRatios: [[1]],
  };
}

function normalizeTerminal(value: unknown, workspaceCwd: string): TerminalSession | null {
  if (!value || typeof value !== 'object') return null;
  const item = value as Partial<TerminalSession>;
  if (typeof item.id !== 'string' || typeof item.name !== 'string') return null;
  return {
    id: item.id,
    name: item.name.trim() || 'Terminal',
    cwd: typeof item.cwd === 'string' && item.cwd ? item.cwd : workspaceCwd,
  };
}

function normalizeWorkspace(value: unknown): Workspace | null {
  if (!value || typeof value !== 'object') return null;
  const item = value as Partial<Workspace>;
  if (typeof item.id !== 'string' || typeof item.name !== 'string') return null;
  const cwd = typeof item.cwd === 'string' ? item.cwd : '';
  const terminals = Array.isArray(item.terminals)
    ? item.terminals
        .map((terminal) => normalizeTerminal(terminal, cwd))
        .filter((terminal): terminal is TerminalSession => terminal !== null)
    : [];
  const rows = distributeRows(terminals);
  const splitRatios = normalizeStoredSplitRatios(rows, item.splitRatios, item.splitRatio);

  return {
    id: item.id,
    name: item.name.trim() || 'Untitled workspace',
    cwd,
    terminals,
    activeTerminalId: terminals.some((terminal) => terminal.id === item.activeTerminalId)
      ? item.activeTerminalId as string
      : terminals[0]?.id ?? null,
    splitRatios,
  };
}

export function loadWorkspaces(storage: Pick<Storage, 'getItem'> = localStorage): Workspace[] {
  try {
    const raw = storage.getItem(STORAGE_KEY) ?? storage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return [createWorkspace('My workspace', '')];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [createWorkspace('My workspace', '')];
    const workspaces = parsed
      .map(normalizeWorkspace)
      .filter((workspace): workspace is Workspace => workspace !== null);
    return workspaces.length ? workspaces : [createWorkspace('My workspace', '')];
  } catch {
    return [createWorkspace('My workspace', '')];
  }
}

export function distributeRows<T>(items: T[]): T[][] {
  if (items.length === 0) return [];
  let rowCount = 1;
  if (items.length >= 3 && items.length <= 6) rowCount = 2;
  else if (items.length >= 7 && items.length <= 12) rowCount = 3;
  else if (items.length > 12) rowCount = Math.ceil(Math.sqrt(items.length / 1.6));

  rowCount = Math.min(rowCount, items.length);
  const base = Math.floor(items.length / rowCount);
  const extra = items.length % rowCount;
  const rows: T[][] = [];
  let cursor = 0;
  for (let index = 0; index < rowCount; index += 1) {
    const size = base + (index < extra ? 1 : 0);
    rows.push(items.slice(cursor, cursor + size));
    cursor += size;
  }
  return rows;
}

function defaultSplitRatiosForRows<T>(rows: T[][]): number[][] {
  return rows.map((row) => {
    const width = row.length ? 1 / row.length : 1;
    return row.map(() => width);
  });
}

function normalizeRowRatios(rowLength: number, raw: unknown): number[] {
  if (
    !Array.isArray(raw)
    || raw.length !== rowLength
    || raw.some((value) => typeof value !== 'number' || !Number.isFinite(value) || value <= 0)
  ) {
    const width = rowLength ? 1 / rowLength : 1;
    return Array.from({ length: rowLength }, () => width);
  }
  const sum = raw.reduce((total, value) => total + value, 0);
  if (!Number.isFinite(sum) || sum <= 0) {
    const width = rowLength ? 1 / rowLength : 1;
    return Array.from({ length: rowLength }, () => width);
  }
  return raw.map((value) => value / sum);
}

function normalizeStoredSplitRatios(
  rows: TerminalSession[][],
  stored: unknown,
  legacySplitRatio: unknown,
): number[][] {
  if (Array.isArray(stored)) {
    return rows.map((row, rowIndex) => normalizeRowRatios(row.length, stored[rowIndex]));
  }
  if (rows.length === 1 && rows[0]?.length === 2 && typeof legacySplitRatio === 'number' && Number.isFinite(legacySplitRatio)) {
    const left = clampSplitRatio(legacySplitRatio);
    return [[left, 1 - left]];
  }
  return defaultSplitRatiosForRows(rows);
}

export function normalizeSplitRatiosForRows<T>(rows: T[][], stored: unknown): number[][] {
  if (!Array.isArray(stored)) return defaultSplitRatiosForRows(rows);
  return rows.map((row, rowIndex) => normalizeRowRatios(row.length, stored[rowIndex]));
}

export function adjustRowSplitRatios(
  splitRatios: number[][],
  rowIndex: number,
  handleIndex: number,
  boundary: number,
  minPaneRatio = DEFAULT_ROW_MIN_SPLIT,
): number[][] | null {
  const rowRatios = splitRatios[rowIndex];
  if (!rowRatios) return null;
  const leftIndex = handleIndex;
  const rightIndex = handleIndex + 1;
  if (leftIndex < 0 || rightIndex >= rowRatios.length) return null;

  const pairTotal = rowRatios[leftIndex] + rowRatios[rightIndex];
  const min = Math.min(minPaneRatio, pairTotal / 2);
  const leftStart = rowRatios.slice(0, leftIndex).reduce((total, value) => total + value, 0);
  const boundaryMin = leftStart + min;
  const boundaryMax = leftStart + pairTotal - min;
  const clampedBoundary = Math.min(boundaryMax, Math.max(boundaryMin, boundary));
  const nextLeftRatio = clampedBoundary - leftStart;
  const nextRightRatio = pairTotal - nextLeftRatio;
  const nextRowRatios = rowRatios.map((ratio, index) => {
    if (index === leftIndex) return nextLeftRatio;
    if (index === rightIndex) return nextRightRatio;
    return ratio;
  });
  return splitRatios.map((ratios, index) => (
    index === rowIndex ? nextRowRatios : [...ratios]
  ));
}

export function computeTileLayout(
  terminals: TerminalSession[],
  splitRatios?: number[][],
): TileLayout {
  const rows = distributeRows(terminals);
  const normalizedSplitRatios = normalizeSplitRatiosForRows(rows, splitRatios);
  const styles: Record<string, string> = {};
  const handles: TileHandle[] = [];
  const rowHeight = rows.length ? 100 / rows.length : 100;
  rows.forEach((row, rowIndex) => {
    const ratios = normalizedSplitRatios[rowIndex];
    const top = rowIndex * rowHeight;
    let left = 0;
    row.forEach((terminal, columnIndex) => {
      const width = ratios[columnIndex] * 100;
      const leftPercent = left * 100;
      styles[terminal.id] = [
        `top: calc(${top}% + 4px)`,
        `left: calc(${leftPercent}% + 4px)`,
        `width: calc(${width}% - 8px)`,
        `height: calc(${rowHeight}% - 8px)`,
      ].join(';');
      left += ratios[columnIndex];
      if (columnIndex < row.length - 1) {
        handles.push({
          rowIndex,
          handleIndex: columnIndex,
          leftPercent: left * 100,
          topPercent: top,
          heightPercent: rowHeight,
        });
      }
    });
  });
  return {
    styles,
    handles,
    rowSplitRatios: normalizedSplitRatios,
  };
}

export function computeTileStyles(
  terminals: TerminalSession[],
  splitRatios?: number[][],
): Record<string, string> {
  return computeTileLayout(terminals, splitRatios).styles;
}

export function moveTerminal(
  workspaces: Workspace[],
  terminalId: string,
  sourceWorkspaceId: string,
  targetWorkspaceId: string,
): Workspace[] {
  if (sourceWorkspaceId === targetWorkspaceId) return workspaces;
  const source = workspaces.find((workspace) => workspace.id === sourceWorkspaceId);
  const terminal = source?.terminals.find((item) => item.id === terminalId);
  if (!source || !terminal) return workspaces;

  return workspaces.map((workspace) => {
    if (workspace.id === sourceWorkspaceId) {
      const index = workspace.terminals.findIndex((item) => item.id === terminalId);
      const terminals = workspace.terminals.filter((item) => item.id !== terminalId);
      return {
        ...workspace,
        terminals,
        activeTerminalId: workspace.activeTerminalId === terminalId
          ? terminals[Math.min(index, terminals.length - 1)]?.id ?? null
          : workspace.activeTerminalId,
      };
    }
    if (workspace.id === targetWorkspaceId) {
      return {
        ...workspace,
        terminals: [...workspace.terminals, terminal],
        activeTerminalId: terminal.id,
      };
    }
    return workspace;
  });
}
