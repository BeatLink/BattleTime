import { describe, expect, it } from '@jest/globals';

import { MockPackages } from '@/ble/mock-packages';
import { bundledPackages } from '@/packages/bundled';
import { buildCatalog, decodeBundle } from '@/packages/catalog';
import {
  installPackage,
  listPackages,
  readAbi,
  setEnabled,
  uninstallPackage,
  type PackageChannel,
} from '@/protocol/transfer';
import type { PackageReply } from '@/protocol/packages';

// Drive the real driver against the mock watch, the way the app does.
class MockChannel implements PackageChannel {
  private watch = new MockPackages();
  private queue: PackageReply[] = [];

  async send(text: string) {
    const replies = this.watch.handle(text) ?? [];
    for (const line of replies) {
      this.queue.push(JSON.parse(line));
    }
  }

  async next(): Promise<PackageReply> {
    const reply = this.queue.shift();
    if (!reply) throw new Error('no reply');
    return reply;
  }
}

describe('install against the mock watch', () => {
  it('installs every bundled package and lists it back', async () => {
    const channel = new MockChannel();
    const abi = await readAbi(channel);

    for (const pkg of bundledPackages) {
      await installPackage(channel, decodeBundle(pkg), { abi });
    }

    const installed = await listPackages(channel);
    expect(installed.map((p) => p.name).sort()).toEqual(
      bundledPackages.map((p) => p.name).sort(),
    );
  });

  it('enables, then removes, and the catalogue follows', async () => {
    const channel = new MockChannel();
    const abi = await readAbi(channel);
    const pkg = bundledPackages[0];

    await installPackage(channel, decodeBundle(pkg), { abi });
    await setEnabled(channel, pkg.name, true);

    let catalog = buildCatalog(await listPackages(channel), abi);
    let entry = catalog.find((e) => e.name === pkg.name)!;
    expect(entry.installed).toBe(true);
    expect(entry.enabled).toBe(true);

    await uninstallPackage(channel, pkg.name);

    catalog = buildCatalog(await listPackages(channel), abi);
    entry = catalog.find((e) => e.name === pkg.name)!;
    expect(entry.installed).toBe(false);
  });
});
