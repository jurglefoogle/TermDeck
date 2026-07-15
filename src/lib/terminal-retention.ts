export const MAX_COMMAND_HISTORY = 1_000;
export const MIN_SCROLLBACK_LINES = 100;
export const MAX_SCROLLBACK_LINES = 50_000;
export const DEFAULT_SCROLLBACK_LINES = 5_000;

export function clampScrollbackLines(value: unknown): number {
  if (typeof value !== 'number' || !Number.isInteger(value)) return DEFAULT_SCROLLBACK_LINES;
  return Math.min(MAX_SCROLLBACK_LINES, Math.max(MIN_SCROLLBACK_LINES, value));
}

export function trimRetainedLines(lines: readonly string[], limit: number): string[] {
  return lines.slice(-Math.max(0, Math.floor(limit)));
}

export function appendCommandHistory(history: readonly string[], command: string): string[] {
  const value = command.trim();
  if (!value || history.at(-1) === value) return [...history];
  return [...history, value].slice(-MAX_COMMAND_HISTORY);
}

export function retainedStringLines(value: unknown, limit: number): string[] {
  if (!Array.isArray(value) || value.some((line) => typeof line !== 'string')) return [];
  return trimRetainedLines(value, limit);
}
