import { describe, expect, it } from 'vitest';
import {
  clampTerminalFontSize,
  DEFAULT_SETTINGS,
  MAX_TERMINAL_FONT_SIZE,
  MIN_TERMINAL_FONT_SIZE,
  SETTINGS_STORAGE_KEY,
  loadSettings,
} from './settings';

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
      terminalFontSize: 13,
      sidebarCollapsed: false,
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

  it('clamps the terminal font size', () => {
    expect(clampTerminalFontSize(MIN_TERMINAL_FONT_SIZE - 1)).toBe(MIN_TERMINAL_FONT_SIZE);
    expect(clampTerminalFontSize(MAX_TERMINAL_FONT_SIZE + 1)).toBe(MAX_TERMINAL_FONT_SIZE);
    expect(clampTerminalFontSize(12.6)).toBe(13);
  });
});
