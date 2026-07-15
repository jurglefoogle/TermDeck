export type AppSettings = {
  retainCommandHistory: boolean;
  retainScrollback: boolean;
};

export const SETTINGS_STORAGE_KEY = 'termdeck.settings.v1';

export const DEFAULT_SETTINGS: AppSettings = {
  retainCommandHistory: false,
  retainScrollback: false,
};

export function loadSettings(storage: Pick<Storage, 'getItem'> = localStorage): AppSettings {
  try {
    const raw = storage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    return {
      retainCommandHistory: parsed.retainCommandHistory === true,
      retainScrollback: parsed.retainScrollback === true,
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

