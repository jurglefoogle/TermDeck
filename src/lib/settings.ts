import { clampScrollbackLines, DEFAULT_SCROLLBACK_LINES } from './terminal-retention';

export type AppSettings = {
  retainCommandHistory: boolean;
  retainScrollback: boolean;
  scrollbackLines: number;
  terminalFontSize: number;
  sidebarCollapsed: boolean;
};

export const SETTINGS_STORAGE_KEY = 'termdeck.settings.v1';

export const DEFAULT_SETTINGS: AppSettings = {
  retainCommandHistory: false,
  retainScrollback: false,
  scrollbackLines: DEFAULT_SCROLLBACK_LINES,
  terminalFontSize: 13,
  sidebarCollapsed: false,
};

export const MIN_TERMINAL_FONT_SIZE = 9;
export const MAX_TERMINAL_FONT_SIZE = 18;

export function clampTerminalFontSize(value: number): number {
  return Math.min(MAX_TERMINAL_FONT_SIZE, Math.max(MIN_TERMINAL_FONT_SIZE, Math.round(value)));
}

export function loadSettings(storage: Pick<Storage, 'getItem'> = localStorage): AppSettings {
  try {
    const raw = storage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    return {
      retainCommandHistory: parsed.retainCommandHistory === true,
      retainScrollback: parsed.retainScrollback === true,
      scrollbackLines: clampScrollbackLines(parsed.scrollbackLines),
      terminalFontSize: clampTerminalFontSize(
        typeof parsed.terminalFontSize === 'number' ? parsed.terminalFontSize : DEFAULT_SETTINGS.terminalFontSize,
      ),
      sidebarCollapsed: parsed.sidebarCollapsed === true,
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}
