import { describe, expect, it } from '@jest/globals';

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { runShellCommand } = require('./remote-command');

describe('running a remote button command', () => {
  it('gives the exit code and what the command printed on either stream', async () => {
    await expect(runShellCommand('echo out; echo err >&2')).resolves.toEqual({ exitCode: 0, output: 'out\nerr\n', timedOut: false });
  });

  it('gives a failing exit code', async () => {
    await expect(runShellCommand('exit 3')).resolves.toMatchObject({ exitCode: 3, timedOut: false });
  });

  it('stops a command that runs too long', async () => {
    const started = Date.now();
    await expect(runShellCommand('echo started; sleep 10', 300)).resolves.toMatchObject({ exitCode: null, timedOut: true });
    expect(Date.now() - started).toBeLessThan(5000);
  });

  it('keeps only the end of a long output', async () => {
    const result = await runShellCommand('seq 1 5000');
    expect(result.output.length).toBe(4000);
    expect(result.output.endsWith('5000\n')).toBe(true);
  });
});
