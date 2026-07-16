import { describe, expect, it } from 'vitest';
import {
  DEFAULT_SCROLLBACK_LINES,
  MAX_COMMAND_LENGTH,
  MAX_SCROLLBACK_LINES,
  MIN_SCROLLBACK_LINES,
  appendCommandHistory,
  clampScrollbackLines,
  parseCommandReport,
  retainedStringLines,
  trimRetainedLines,
} from './terminal-retention';

const encode = (value: string) => btoa(String.fromCharCode(...new TextEncoder().encode(value)));

describe('terminal retention', () => {
  it('clamps the configured scrollback limit to safe bounds', () => {
    expect(clampScrollbackLines(undefined)).toBe(DEFAULT_SCROLLBACK_LINES);
    expect(clampScrollbackLines(1)).toBe(MIN_SCROLLBACK_LINES);
    expect(clampScrollbackLines(100_000)).toBe(MAX_SCROLLBACK_LINES);
    expect(clampScrollbackLines(750)).toBe(750);
  });

  it('keeps only the most recent scrollback lines', () => {
    expect(trimRetainedLines(['one', 'two', 'three'], 2)).toEqual(['two', 'three']);
  });

  it('records non-empty commands without repeated adjacent entries', () => {
    expect(appendCommandHistory(['pwd'], 'pwd')).toEqual(['pwd']);
    expect(appendCommandHistory(['pwd'], '  cd src  ')).toEqual(['pwd', 'cd src']);
    expect(appendCommandHistory(['pwd'], '   ')).toEqual(['pwd']);
  });

  it('rejects malformed persisted lines', () => {
    expect(retainedStringLines(['one', 2], 500)).toEqual([]);
    expect(retainedStringLines(['one', 'two'], 1)).toEqual(['two']);
  });

  it('decodes a command reported by the shell', () => {
    expect(parseCommandReport(encode('ls -la'))).toBe('ls -la');
    expect(parseCommandReport(encode('  cd /tmp  '))).toBe('cd /tmp');
    expect(parseCommandReport(encode('echo "héllo → ☃"'))).toBe('echo "héllo → ☃"');
  });

  it('ignores reports that are empty or malformed', () => {
    expect(parseCommandReport('')).toBeNull();
    expect(parseCommandReport(encode('   '))).toBeNull();
    expect(parseCommandReport('not base64 !!!')).toBeNull();
    expect(parseCommandReport(encode('bad\0command'))).toBeNull();
  });

  it('ignores an oversized report rather than storing it', () => {
    expect(parseCommandReport(encode('x'.repeat(MAX_COMMAND_LENGTH + 1)))).toBeNull();
    expect(parseCommandReport(encode('x'.repeat(MAX_COMMAND_LENGTH)))).toHaveLength(MAX_COMMAND_LENGTH);
  });
});
