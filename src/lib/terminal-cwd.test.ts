import { describe, expect, it } from 'vitest';
import { parseOsc7Cwd } from './terminal-cwd';

describe('OSC 7 current-directory parser', () => {
  it('reads a Windows file URI without its URI slash', () => {
    expect(parseOsc7Cwd('file:///C:/Users/dennis.eastman/source')).toBe('C:/Users/dennis.eastman/source');
  });

  it('reads and decodes a Linux file URI', () => {
    expect(parseOsc7Cwd('file://localhost/home/dennis/My%20Project')).toBe('/home/dennis/My Project');
  });

  it('rejects unsupported or malformed OSC 7 data', () => {
    expect(parseOsc7Cwd('https://example.com')).toBeNull();
    expect(parseOsc7Cwd('file://localhost')).toBeNull();
    expect(parseOsc7Cwd('file:///bad%path')).toBeNull();
  });
});

