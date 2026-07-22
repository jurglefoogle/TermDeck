import { clampScrollbackLines, DEFAULT_SCROLLBACK_LINES } from './terminal-retention';

export type AppSettings = {
  retainCommandHistory: boolean;
  retainScrollback: boolean;
  scrollbackLines: number;
};

export const SETTINGS_STORAGE_KEY = 'termdeck.settings.v1';

export const DEFAULT_SETTINGS: AppSettings = {
  retainCommandHistory: false,
  retainScrollback: false,
  scrollbackLines: DEFAULT_SCROLLBACK_LINES,
};

export function loadSettings(storage: Pick<Storage, 'getItem'> = localStorage): AppSettings {
  try {
    const raw = storage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    return {
      retainCommandHistory: parsed.retainCommandHistory === true,
      retainScrollback: parsed.retainScrollback === true,
      scrollbackLines: clampScrollbackLines(parsed.scrollbackLines),
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}
