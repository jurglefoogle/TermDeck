import type { TerminalSession, Workspace } from './types';

export const STORAGE_KEY = 'termdeck.workspaces.v2';
export const LEGACY_STORAGE_KEY = 'termdeck.workspaces.v1';
export const ACTIVE_WORKSPACE_KEY = 'termdeck.active-workspace.v2';

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
  return {
    id: item.id,
    name: item.name.trim() || 'Untitled workspace',
    cwd,
    terminals,
    activeTerminalId: terminals.some((terminal) => terminal.id === item.activeTerminalId)
      ? item.activeTerminalId as string
      : terminals[0]?.id ?? null,
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

export function computeTileStyles(terminals: TerminalSession[]): Record<string, string> {
  const rows = distributeRows(terminals);
  const styles: Record<string, string> = {};
  rows.forEach((row, rowIndex) => {
    row.forEach((terminal, columnIndex) => {
      const top = (rowIndex * 100) / rows.length;
      const left = (columnIndex * 100) / row.length;
      const width = 100 / row.length;
      const height = 100 / rows.length;
      styles[terminal.id] = [
        `top: calc(${top}% + 4px)`,
        `left: calc(${left}% + 4px)`,
        `width: calc(${width}% - 8px)`,
        `height: calc(${height}% - 8px)`,
      ].join(';');
    });
  });
  return styles;
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

