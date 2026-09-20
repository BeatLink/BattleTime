// Everything the tool prints, kept in one place so the commands stay about
// the watch rather than about formatting.

export function say(text: string) {
  process.stdout.write(`${text}\n`);
}

export function warn(text: string) {
  process.stderr.write(`${text}\n`);
}

export function kilobytes(bytes: number): string {
  return `${Math.round(bytes / 1024)} kB`;
}

// A progress bar that redraws in place, and gives up on that when the output
// is a file or a pipe rather than a terminal.
export class ProgressBar {
  private lastLine = '';

  constructor(private readonly width = 40) {}

  update(done: number, total: number, label: string) {
    const fraction = total > 0 ? Math.min(1, done / total) : 0;
    const filled = Math.round(this.width * fraction);
    const bar = '#'.repeat(filled) + '.'.repeat(this.width - filled);
    const line = `[${bar}] ${Math.round(fraction * 100)}% ${label}`;
    if (line === this.lastLine) {
      return;
    }
    this.lastLine = line;
    if (process.stdout.isTTY) {
      process.stdout.write(`\r${line}\u001b[K`);
    } else {
      process.stdout.write(`${line}\n`);
    }
  }

  done() {
    if (process.stdout.isTTY && this.lastLine) {
      process.stdout.write('\n');
    }
    this.lastLine = '';
  }
}
