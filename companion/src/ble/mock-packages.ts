// A small stand-in for wasp/pkgmgr.py, so the Apps tab works against the mock
// watch on web and in Expo Go. It keeps packages in memory and answers with the
// same JSON the firmware sends.

import { bytesToBase64 } from './encoding';

type Installed = { name: string; version: string; enabled: boolean; kind: 'app' | 'face' };

// Current firmware, which offers raw transfer in 512 byte windows.
const ABI = { mpy: 6, arch: 0, raw: true, window: 512 };

export class MockPackages {
  private installed = new Map<string, Installed>();
  // Files on the mock flash, seeded with what a watch that has been worn a while holds.
  private files = new Map<string, number[]>([
    ['settings.json', [...'{"brightness": 2, "notify_level": 2}'].map((c) => c.charCodeAt(0))],
    ['alarms.txt', [...'7,30,159;'].map((c) => c.charCodeAt(0))],
  ]);
  private receiving: { path: string; size: number; raw: boolean; got: number[] } | null = null;

  // Handle one line from the phone. Returns the replies to emit, or null when
  // the line is not for the package manager.
  handle(text: string): string[] | null {
    if (this.receiving && !this.receiving.raw) {
      return this.accept(decodeBase64(text.trim()));
    }

    const trimmed = text.trim();
    if (!trimmed.startsWith('pkg.')) {
      return null;
    }

    const recv = /^pkg\.recv\("([^"]+)", (\d+), (True|False)\)/.exec(trimmed);
    if (recv) {
      const size = Number(recv[2]);
      if (size === 0) {
        this.files.set(recv[1], []);
        return [reply({ ok: true, rx: 0 }), reply({ ok: true, got: 0, sum: 0 })];
      }
      this.receiving = { path: recv[1], size, raw: recv[3] === 'False', got: [] };
      return [reply({ ok: true, rx: size })];
    }

    const lsDir = /^pkg\.ls_dir\("([^"]*)"\)/.exec(trimmed);
    if (lsDir) {
      return [this.listDir(lsDir[1])];
    }

    const send = /^pkg\.send\("([^"]+)"\)/.exec(trimmed);
    if (send) {
      return this.sendFile(send[1]);
    }

    const rmFile = /^pkg\.rm_file\("([^"]+)"\)/.exec(trimmed);
    if (rmFile) {
      if (!this.files.delete(rmFile[1])) {
        return [reply({ ok: false, err: 'no such file' })];
      }
      return [reply({ ok: true, deleted: rmFile[1] })];
    }

    if (trimmed.startsWith('pkg.mem()')) {
      return [reply({ ok: true, free: 19152, alloc: 45648 })];
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

  private listDir(dir: string): string {
    const prefix = dir ? `${dir}/` : '';
    const entries = new Map<string, [string, number, boolean]>();
    for (const [path, bytes] of this.files) {
      if (!path.startsWith(prefix)) {
        continue;
      }
      const rest = path.slice(prefix.length);
      const slash = rest.indexOf('/');
      if (slash < 0) {
        entries.set(rest, [rest, bytes.length, false]);
      } else {
        const name = rest.slice(0, slash);
        entries.set(name, [name, 0, true]);
      }
    }
    if (dir && entries.size === 0) {
      return reply({ ok: false, err: 'no such directory' });
    }
    return reply({ ok: true, dir, entries: [...entries.values()] });
  }

  private sendFile(path: string): string[] {
    const bytes = this.files.get(path);
    if (!bytes) {
      return [reply({ ok: false, err: 'no such file' })];
    }
    const replies = [reply({ ok: true, tx: bytes.length })];
    for (let off = 0; off < bytes.length; off += 192) {
      replies.push(reply({ off, d: bytesToBase64(Uint8Array.from(bytes.slice(off, off + 192))) }));
    }
    replies.push(
      reply({ ok: true, got: bytes.length, sum: bytes.reduce((total, byte) => (total + byte) >>> 0, 0) }),
    );
    return replies;
  }

  // Handle bytes sent as they are, which only a raw transfer expects.
  handleBytes(data: Uint8Array): string[] {
    if (!this.receiving?.raw) {
      return [];
    }
    return this.accept([...data]);
  }

  private accept(chunk: number[]): string[] {
    const transfer = this.receiving as { path: string; size: number; got: number[] };
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
