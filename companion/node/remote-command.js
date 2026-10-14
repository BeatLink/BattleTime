// Runs a remote button's shell command on this computer, for the desktop app and the command line tool.
//
// The command comes only from the user's own button settings, never from the watch.

const { exec } = require('node:child_process');

// How long a command may run before it is stopped.
const COMMAND_TIMEOUT_MS = 30000;

// The most output kept from a command, from its end, since only a status line shows it.
const OUTPUT_LIMIT = 4000;

// Runs `command` in the user's shell and resolves with its exit code, its output and whether it ran out of time.
function runShellCommand(command, timeoutMs = COMMAND_TIMEOUT_MS) {
  return new Promise((resolve) => {
    exec(command, { timeout: timeoutMs, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024 }, (error, stdout, stderr) => {
      const output = `${stdout}${stderr}`.slice(-OUTPUT_LIMIT);
      const timedOut = Boolean(error && error.killed);
      let exitCode = 0;
      if (error) {
        exitCode = typeof error.code === 'number' ? error.code : null;
      }
      resolve({ exitCode: timedOut ? null : exitCode, output, timedOut });
    });
  });
}

module.exports = { runShellCommand, COMMAND_TIMEOUT_MS };
