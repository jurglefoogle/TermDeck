export const MAX_COMMAND_HISTORY = 1_000;
export const MIN_SCROLLBACK_LINES = 100;
export const MAX_SCROLLBACK_LINES = 50_000;
export const DEFAULT_SCROLLBACK_LINES = 5_000;

/**
 * OSC identifier the shell integration uses to report a command the shell
 * itself recorded in its history. Reading commands from the shell rather than
 * sniffing keystrokes keeps secrets typed at a child process's prompt (ssh,
 * sudo) out of the history entirely: they never reach the shell.
 */
export const COMMAND_REPORT_OSC = 6973;
export const MAX_COMMAND_LENGTH = 4_096;

export function parseCommandReport(data: string): string | null {
  if (!data || data.length > MAX_COMMAND_LENGTH * 2) return null;
  let decoded: string;
  try {
    const binary = atob(data);
    const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    decoded = new TextDecoder().decode(bytes);
  } catch {
    return null;
  }
  const command = decoded.trim();
  if (!command || command.includes('\0') || command.length > MAX_COMMAND_LENGTH) return null;
  return command;
}

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
