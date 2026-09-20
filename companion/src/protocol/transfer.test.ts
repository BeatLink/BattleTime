import { describe, expect, it } from '@jest/globals';

import { checksum, type AbiInfo, type PackageBundle, type PackageReply } from './packages';
import {
  installPackage,
  listPackages,
  readAbi,
  sendFile,
  setEnabled,
  TransferError,
  uninstallPackage,
  writeConfig,
  type PackageChannel,
} from './transfer';

const abi: AbiInfo = { mpy: 6, arch: 0, raw: false, window: 96 };

// A watch that behaves, so the driver can be exercised end to end.
class FakeWatch implements PackageChannel {
  sent: string[] = [];
  files = new Map<string, Uint8Array>();
  private replies: PackageReply[] = [];
  private pending: ((reply: PackageReply) => void)[] = [];
  private receiving: { path: string; size: number; got: number[] } | null = null;

  constructor(private readonly index: Record<string, unknown>[] = []) {}

  async send(text: string) {
    this.sent.push(text);

    if (this.receiving) {
      const chunk = Buffer.from(text.trim(), 'base64');
      this.receiving.got.push(...chunk);
      this.push({ t: 'pkg', ack: this.receiving.got.length });
      if (this.receiving.got.length >= this.receiving.size) {
        const data = Uint8Array.from(this.receiving.got);
        this.files.set(this.receiving.path, data);
        this.push({ t: 'pkg', ok: true, got: data.length, sum: checksum(data) });
        this.receiving = null;
      }
      return;
    }

    const recv = /^pkg\.recv\("([^"]+)", (\d+), (True|False)\)/.exec(text);
    if (recv) {
      const size = Number(recv[2]);
      this.push({ t: 'pkg', ok: true, rx: size });
      if (size === 0) {
        // pkgmgr's receive loop never runs, so the result follows at once.
        this.files.set(recv[1], new Uint8Array());
        this.push({ t: 'pkg', ok: true, got: 0, sum: 0 });
        return;
      }
      this.receiving = { path: recv[1], size, got: [] };
      return;
    }
    if (text.startsWith('pkg.abi()')) {
      this.push({ t: 'pkg', ok: true, abi });
      return;
    }
    if (text.startsWith('pkg.ls()')) {
      this.push({ t: 'pkg', ok: true, pkgs: this.index as never });
      return;
    }
    if (text.startsWith('pkg.reindex()')) {
      this.push({ t: 'pkg', ok: true, count: this.files.size });
      return;
    }
    const rm = /^pkg\.rm\("([^"]+)"\)/.exec(text);
    if (rm) {
      this.push({ t: 'pkg', ok: true, removed: rm[1] });
      return;
    }
    const toggle = /^pkg\.(enable|disable)\("([^"]+)"\)/.exec(text);
    if (toggle) {
      this.push({ t: 'pkg', ok: true, name: toggle[2], enabled: toggle[1] === 'enable' });
      return;
    }
    const cfg = /^pkg\.cfg\("([^"]+)", (.*)\)/.exec(text);
    if (cfg) {
      this.push({ t: 'pkg', ok: true, name: cfg[1] });
      return;
    }
  }

  async next(): Promise<PackageReply> {
    const ready = this.replies.shift();
    if (ready) {
      return ready;
    }
    return new Promise((resolve) => this.pending.push(resolve));
  }

  push(reply: PackageReply) {
    const waiting = this.pending.shift();
    if (waiting) {
      waiting(reply);
    } else {
      this.replies.push(reply);
    }
  }
}

// A watch that says no, to drive the failure paths.
class RudeWatch implements PackageChannel {
  constructor(private readonly replies: PackageReply[]) {}
  sent: string[] = [];
  async send(text: string) {
    this.sent.push(text);
  }
  async next(): Promise<PackageReply> {
    const reply = this.replies.shift();
    if (!reply) {
      throw new Error('the test ran out of replies');
    }
    return reply;
  }
}

function payload(size: number): Uint8Array {
  const data = new Uint8Array(size);
  for (let i = 0; i < size; i++) {
    data[i] = i % 256;
  }
  return data;
}

describe('sendFile', () => {
  it('delivers the bytes intact', async () => {
    const watch = new FakeWatch();
    const data = payload(500);

    await sendFile(watch, 'pkg/calculator/app.mpy', data, { abi });

    expect(watch.files.get('pkg/calculator/app.mpy')).toEqual(data);
  });

  it('splits the file into windows the watch asked for', async () => {
    const watch = new FakeWatch();

    await sendFile(watch, 'pkg/a/app.mpy', payload(250), { abi });

    const chunks = watch.sent.filter((line) => !line.startsWith('pkg.'));
    expect(chunks).toHaveLength(3);
  });

  it('honours a smaller window than the default', async () => {
    const watch = new FakeWatch();

    await sendFile(watch, 'pkg/a/app.mpy', payload(100), {
      abi: { ...abi, window: 32 },
    });

    const chunks = watch.sent.filter((line) => !line.startsWith('pkg.'));
    expect(chunks).toHaveLength(4);
  });

  it('reports progress as each window lands', async () => {
    const watch = new FakeWatch();
    const seen: number[] = [];

    await sendFile(watch, 'pkg/a/app.mpy', payload(250), {
      abi,
      onProgress: (progress) => seen.push(progress.sent),
    });

    expect(seen).toEqual([96, 192, 250]);
  });

  it('sends an empty file without any chunks', async () => {
    const watch = new FakeWatch();

    await sendFile(watch, 'pkg/a/empty', new Uint8Array(), { abi });

    expect(watch.files.get('pkg/a/empty')).toEqual(new Uint8Array());
  });

  it('fails when the watch rejects the transfer', async () => {
    const watch = new RudeWatch([{ t: 'pkg', ok: false, err: 'raw unavailable, use b64' }]);

    await expect(sendFile(watch, 'pkg/a/app.mpy', payload(10), { abi })).rejects.toThrow(
      /raw unavailable/,
    );
  });

  it('fails when the watch acknowledges the wrong offset', async () => {
    const watch = new RudeWatch([
      { t: 'pkg', ok: true, rx: 10 },
      { t: 'pkg', ack: 4 },
    ]);

    await expect(sendFile(watch, 'pkg/a/app.mpy', payload(10), { abi })).rejects.toThrow(
      TransferError,
    );
  });

  it('fails when the checksum does not match', async () => {
    const data = payload(10);
    const watch = new RudeWatch([
      { t: 'pkg', ok: true, rx: 10 },
      { t: 'pkg', ack: 10 },
      { t: 'pkg', ok: true, got: 10, sum: checksum(data) + 1 },
    ]);

    await expect(sendFile(watch, 'pkg/a/app.mpy', data, { abi })).rejects.toThrow(/checksum/);
  });

  it('fails when the watch received fewer bytes than sent', async () => {
    const data = payload(10);
    const watch = new RudeWatch([
      { t: 'pkg', ok: true, rx: 10 },
      { t: 'pkg', ack: 10 },
      { t: 'pkg', ok: true, got: 5, sum: checksum(data) },
    ]);

    await expect(sendFile(watch, 'pkg/a/app.mpy', data, { abi })).rejects.toThrow(/5 of 10/);
  });
});

describe('installPackage', () => {
  const bundle: PackageBundle = {
    name: 'calculator',
    meta: {
      name: 'calculator',
      cls: 'CalculatorApp',
      label: 'Calculator',
      version: '1.0.0',
      kind: 'app',
      resident: false,
      quick_ring: false,
      icon: true,
      abi: { mpy: 6, arch: 0 },
    },
    files: [
      { path: 'app.mpy', data: payload(300) },
      { path: 'icon.rle', data: payload(120) },
      { path: 'meta.json', data: payload(60) },
    ],
  };

  it('writes every file under the package directory', async () => {
    const watch = new FakeWatch();

    await installPackage(watch, bundle, { abi });

    expect([...watch.files.keys()].sort()).toEqual([
      'pkg/calculator/app.mpy',
      'pkg/calculator/icon.rle',
      'pkg/calculator/meta.json',
    ]);
  });

  it('rebuilds the index once everything has landed', async () => {
    const watch = new FakeWatch();

    await installPackage(watch, bundle, { abi });

    expect(watch.sent.some((line) => line.startsWith('pkg.reindex()'))).toBe(true);
  });

  it('refuses a package the watch cannot load, before sending anything', async () => {
    const watch = new FakeWatch();
    const stale = { ...bundle, meta: { ...bundle.meta, abi: { mpy: 5, arch: 0 } } };

    await expect(installPackage(watch, stale, { abi })).rejects.toThrow(/bytecode 5/);
    expect(watch.sent).toHaveLength(0);
  });
});

describe('the other commands', () => {
  it('reads the watch ABI', async () => {
    expect(await readAbi(new FakeWatch())).toEqual(abi);
  });

  it('lists installed packages', async () => {
    const watch = new FakeWatch([
      { name: 'calculator', version: '1.0.0', enabled: true, kind: 'app' },
    ]);

    const packages = await listPackages(watch);

    expect(packages).toHaveLength(1);
    expect(packages[0].name).toBe('calculator');
  });

  it('uninstalls', async () => {
    const watch = new FakeWatch();
    await expect(uninstallPackage(watch, 'calculator')).resolves.toBeUndefined();
  });

  it('enables and disables', async () => {
    const watch = new FakeWatch();
    await setEnabled(watch, 'calculator', true);
    await setEnabled(watch, 'calculator', false);
    expect(watch.sent).toEqual([
      'pkg.enable("calculator")\r\n',
      'pkg.disable("calculator")\r\n',
    ]);
  });

  it('writes config', async () => {
    const watch = new FakeWatch();
    await writeConfig(watch, 'calculator', { loud: true });
    expect(watch.sent[0]).toContain('pkg.cfg("calculator"');
  });

  it('surfaces an error from a command', async () => {
    const watch = new RudeWatch([{ t: 'pkg', ok: false, err: 'not installed' }]);
    await expect(uninstallPackage(watch, 'nope')).rejects.toThrow(/not installed/);
  });
});
