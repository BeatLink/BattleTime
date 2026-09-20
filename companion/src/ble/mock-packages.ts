// A small stand-in for wasp/pkgmgr.py, so the Apps tab works against the mock
// watch on web and in Expo Go. It keeps packages in memory and answers with the
// same JSON the firmware sends.

type Installed = { name: string; version: string; enabled: boolean; kind: 'app' | 'face' };

const ABI = { mpy: 6, arch: 0, raw: false, window: 96 };

export class MockPackages {
  private installed = new Map<string, Installed>();
  private files = new Map<string, number[]>();
  private receiving: { path: string; size: number; got: number[] } | null = null;

  // Handle one line from the phone. Returns the replies to emit, or null when
  // the line is not for the package manager.
  handle(text: string): string[] | null {
    if (this.receiving) {
      return this.receiveChunk(text);
    }

    const trimmed = text.trim();
    if (!trimmed.startsWith('pkg.')) {
      return null;
    }

    const recv = /^pkg\.recv\("([^"]+)", (\d+), (?:True|False)\)/.exec(trimmed);
    if (recv) {
      const size = Number(recv[2]);
      if (size === 0) {
        this.files.set(recv[1], []);
        return [reply({ ok: true, rx: 0 }), reply({ ok: true, got: 0, sum: 0 })];
      }
      this.receiving = { path: recv[1], size, got: [] };
      return [reply({ ok: true, rx: size })];
    }

    if (trimmed.startsWith('pkg.abi()')) {
      return [reply({ ok: true, abi: ABI })];
    }

    if (trimmed.startsWith('pkg.ls()')) {
      return [reply({ ok: true, pkgs: [...this.installed.values()] })];
    }

    if (trimmed.startsWith('pkg.reindex()')) {
      this.indexFromFiles();
      return [reply({ ok: true, count: this.installed.size })];
    }

    const rm = /^pkg\.rm\("([^"]+)"\)/.exec(trimmed);
    if (rm) {
      if (!this.installed.has(rm[1])) {
        return [reply({ ok: false, err: 'not installed' })];
      }
      this.installed.delete(rm[1]);
      for (const path of [...this.files.keys()]) {
        if (path.startsWith(`pkg/${rm[1]}/`)) {
          this.files.delete(path);
        }
      }
      return [reply({ ok: true, removed: rm[1] })];
    }

    const toggle = /^pkg\.(enable|disable)\("([^"]+)"\)/.exec(trimmed);
    if (toggle) {
      const entry = this.installed.get(toggle[2]);
      if (!entry) {
        return [reply({ ok: false, err: 'not installed' })];
      }
      entry.enabled = toggle[1] === 'enable';
      return [reply({ ok: true, name: entry.name, enabled: entry.enabled })];
    }

    const cfg = /^pkg\.cfg\("([^"]+)",/.exec(trimmed);
    if (cfg) {
      if (!this.installed.has(cfg[1])) {
        return [reply({ ok: false, err: 'not installed' })];
      }
      return [reply({ ok: true, name: cfg[1] })];
    }

    return [reply({ ok: false, err: 'unknown command' })];
  }

  private receiveChunk(text: string): string[] {
    const transfer = this.receiving as { path: string; size: number; got: number[] };
    const chunk = decodeBase64(text.trim());
    transfer.got.push(...chunk);

    const replies = [reply({ ack: transfer.got.length })];
    if (transfer.got.length >= transfer.size) {
      this.files.set(transfer.path, transfer.got);
      replies.push(
        reply({
          ok: true,
          got: transfer.got.length,
          sum: transfer.got.reduce((total, byte) => (total + byte) >>> 0, 0),
        }),
      );
      this.receiving = null;
    }
    return replies;
  }

  // Rebuild the installed list from the manifests that have been written, the
  // way reindex does on the watch.
  private indexFromFiles() {
    for (const [path, bytes] of this.files) {
      const match = /^pkg\/([^/]+)\/meta\.json$/.exec(path);
      if (!match) {
        continue;
      }
      try {
        const meta = JSON.parse(String.fromCharCode(...bytes));
        this.installed.set(match[1], {
          name: match[1],
          version: meta.version ?? '?',
          kind: meta.kind ?? 'app',
          enabled: this.installed.get(match[1])?.enabled ?? false,
        });
      } catch {
        // A manifest that does not parse is skipped, as on the watch.
      }
    }
  }
}

function reply(body: Record<string, unknown>): string {
  return JSON.stringify({ t: 'pkg', ...body });
}

function decodeBase64(text: string): number[] {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  const clean = text.replace(/[^A-Za-z0-9+/]/g, '');
  const bytes: number[] = [];
  for (let i = 0; i < clean.length; i += 4) {
    const quad = [0, 1, 2, 3].map((offset) => chars.indexOf(clean[i + offset] ?? 'A'));
    const triple = (quad[0] << 18) | (quad[1] << 12) | (quad[2] << 6) | quad[3];
    bytes.push((triple >> 16) & 255);
    if (i + 2 < clean.length) bytes.push((triple >> 8) & 255);
    if (i + 3 < clean.length) bytes.push(triple & 255);
  }
  return bytes;
}
