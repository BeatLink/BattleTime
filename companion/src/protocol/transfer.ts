// Drives a file transfer to the watch and the install flow built on top.
//
// The watch acknowledges every window, so the phone never runs further ahead
// than the small buffer between the radio and Python can hold.

import {
  checksum,
  encodeCfg,
  encodeChunk,
  encodeDisable,
  encodeEnable,
  encodeLs,
  encodeRecv,
  encodeReindex,
  encodeRm,
  isCompatible,
  incompatibilityReason,
  type AbiInfo,
  type InstalledPackage,
  type PackageBundle,
  type PackageReply,
} from './packages';

// What the driver needs from the outside world: a way to send a line and a way
// to wait for the next reply. The BLE transport supplies both.
export interface PackageChannel {
  send(text: string): Promise<void>;
  next(): Promise<PackageReply>;
}

export type Progress = {
  file: string;
  sent: number;
  total: number;
};

export class TransferError extends Error {}

// Default window. The watch reports its own, which wins when it is smaller.
const DEFAULT_WINDOW = 96;

function windowFor(abi: AbiInfo | null): number {
  if (!abi || !abi.window) {
    return DEFAULT_WINDOW;
  }
  return Math.max(16, Math.min(abi.window, 512));
}

async function expect(
  channel: PackageChannel,
  check: (reply: PackageReply) => boolean,
  what: string,
): Promise<PackageReply> {
  const reply = await channel.next();
  if (reply.ok === false) {
    throw new TransferError(reply.err ?? `The watch rejected ${what}.`);
  }
  if (!check(reply)) {
    throw new TransferError(`Unexpected reply while ${what}.`);
  }
  return reply;
}

// Send one file, waiting for an acknowledgement after every window.
export async function sendFile(
  channel: PackageChannel,
  path: string,
  data: Uint8Array,
  options: { abi?: AbiInfo | null; onProgress?: (progress: Progress) => void } = {},
): Promise<void> {
  const size = data.length;
  const window = windowFor(options.abi ?? null);

  // Base64 is the mode every firmware has. Raw needs sys.stdin.buffer, which
  // is absent until the board enables it.
  await channel.send(encodeRecv(path, size, true));
  await expect(channel, (reply) => reply.rx === size, `starting ${path}`);

  let sent = 0;
  while (sent < size) {
    const chunk = data.subarray(sent, Math.min(sent + window, size));
    await channel.send(encodeChunk(chunk));
    sent += chunk.length;
    const ack = await expect(channel, (reply) => reply.ack === sent, `sending ${path}`);
    options.onProgress?.({ file: path, sent: ack.ack ?? sent, total: size });
  }

  const final = await channel.next();
  if (final.ok !== true) {
    throw new TransferError(final.err ?? `The watch did not accept ${path}.`);
  }
  if (final.got !== size) {
    throw new TransferError(`${path}: the watch received ${final.got} of ${size} bytes.`);
  }
  if (final.sum !== checksum(data)) {
    throw new TransferError(`${path}: checksum mismatch.`);
  }
}

export async function readAbi(channel: PackageChannel): Promise<AbiInfo> {
  await channel.send('pkg.abi()\r\n');
  const reply = await expect(channel, (r) => !!r.abi, 'reading the watch ABI');
  return reply.abi as AbiInfo;
}

export async function listPackages(channel: PackageChannel): Promise<InstalledPackage[]> {
  await channel.send(encodeLs());
  const reply = await expect(channel, (r) => Array.isArray(r.pkgs), 'listing packages');
  return reply.pkgs as InstalledPackage[];
}

// Transfer every file in a package, then rebuild the index.
export async function installPackage(
  channel: PackageChannel,
  bundle: PackageBundle,
  options: { abi?: AbiInfo | null; onProgress?: (progress: Progress) => void } = {},
): Promise<void> {
  const abi = options.abi ?? null;
  if (abi && !isCompatible(bundle.meta, abi)) {
    throw new TransferError(incompatibilityReason(bundle.meta, abi) as string);
  }

  for (const file of bundle.files) {
    await sendFile(channel, `pkg/${bundle.name}/${file.path}`, file.data, {
      abi,
      onProgress: options.onProgress,
    });
  }

  await channel.send(encodeReindex());
  await expect(channel, (reply) => typeof reply.count === 'number', 'rebuilding the index');
}

export async function uninstallPackage(channel: PackageChannel, name: string): Promise<void> {
  await channel.send(encodeRm(name));
  await expect(channel, (reply) => reply.removed === name, `removing ${name}`);
}

export async function setEnabled(
  channel: PackageChannel,
  name: string,
  enabled: boolean,
): Promise<void> {
  await channel.send(enabled ? encodeEnable(name) : encodeDisable(name));
  await expect(channel, (reply) => reply.enabled === enabled, `updating ${name}`);
}

export async function writeConfig(
  channel: PackageChannel,
  name: string,
  values: Record<string, unknown>,
): Promise<void> {
  await channel.send(encodeCfg(name, values));
  await expect(channel, (reply) => reply.name === name, `configuring ${name}`);
}
