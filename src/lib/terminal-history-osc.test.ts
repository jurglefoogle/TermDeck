import { Terminal } from '@xterm/xterm';
import { describe, expect, it } from 'vitest';
import { COMMAND_REPORT_OSC, parseCommandReport } from './terminal-retention';

const osc = (payload: string) => `\x1b]${COMMAND_REPORT_OSC};${payload}\x07`;
const encode = (value: string) => btoa(String.fromCharCode(...new TextEncoder().encode(value)));

function reportsFrom(chunk: string): Promise<(string | null)[]> {
  const terminal = new Terminal();
  const reports: (string | null)[] = [];
  terminal.parser.registerOscHandler(COMMAND_REPORT_OSC, (data) => {
    reports.push(parseCommandReport(data));
    return true;
  });
  return new Promise((resolve) => {
    terminal.write(chunk, () => {
      terminal.dispose();
      resolve(reports);
    });
  });
}

describe('command report over OSC', () => {
  it('delivers a command the shell emitted to the handler', async () => {
    expect(await reportsFrom(osc(encode('ls -la')))).toEqual(['ls -la']);
  });

  it('handles a report split across writes and mixed with ordinary output', async () => {
    const reports = await reportsFrom(`hello\r\n${osc(encode('cd /tmp'))}world\r\n`);
    expect(reports).toEqual(['cd /tmp']);
  });

  it('does not surface a command for a malformed payload', async () => {
    expect(await reportsFrom(osc('!!!not-base64!!!'))).toEqual([null]);
  });

  it('leaves other OSC sequences alone', async () => {
    // OSC 7 (cwd) must not be picked up as a command report.
    expect(await reportsFrom('\x1b]7;file://host/tmp\x07')).toEqual([]);
  });
});
