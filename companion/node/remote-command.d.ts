// Types for remote-command.js, which the command line tool imports.

export type ShellCommandResult = { exitCode: number | null; output: string; timedOut: boolean };

export const COMMAND_TIMEOUT_MS: number;

export function runShellCommand(command: string, timeoutMs?: number): Promise<ShellCommandResult>;
