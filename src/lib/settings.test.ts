import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS, SETTINGS_STORAGE_KEY, loadSettings } from './settings';

function storageWith(value: string | null): Pick<Storage, 'getItem'> {
  return { getItem: () => value };
}

describe('settings storage', () => {
  it('uses privacy-preserving defaults when no settings are stored', () => {
    expect(loadSettings(storageWith(null))).toEqual(DEFAULT_SETTINGS);
  });

  it('loads explicitly enabled retention preferences', () => {
    expect(loadSettings(storageWith(JSON.stringify({
      retainCommandHistory: true,
      retainScrollback: true,
      scrollbackLines: 500,
    })))).toEqual({
      retainCommandHistory: true,
      retainScrollback: true,
      scrollbackLines: 500,
    });
  });

  it('rejects malformed settings and unknown values', () => {
    expect(loadSettings(storageWith('{invalid'))).toEqual(DEFAULT_SETTINGS);
    expect(loadSettings(storageWith(JSON.stringify({
      retainCommandHistory: 'true',
      retainScrollback: 1,
    })))).toEqual(DEFAULT_SETTINGS);
  });

  it('clamps the stored scrollback line limit', () => {
    expect(loadSettings(storageWith(JSON.stringify({ scrollbackLines: 10 }))).scrollbackLines).toBe(100);
    expect(loadSettings(storageWith(JSON.stringify({ scrollbackLines: 700 }))).scrollbackLines).toBe(700);
  });

  it('uses a versioned storage key', () => {
    expect(SETTINGS_STORAGE_KEY).toBe('termdeck.settings.v1');
  });
});
