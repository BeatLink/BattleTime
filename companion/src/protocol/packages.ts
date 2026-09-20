// Commands and replies for the watch-side package manager, wasp/pkgmgr.py in
// the NeoTime tree. Keep the two in step.
//
// Every command is a line of Python evaluated by the watch REPL, the same
// channel the Gadgetbridge messages use. Every reply is one JSON object
// tagged "pkg".

import { bytesToBase64 } from '@/ble/encoding';

export type AbiInfo = {
  // Bytecode version the firmware can load, and the native architecture.
  mpy: number | null;
  arch: number | null;
  // True when the watch offers raw binary transfer as well as base64.
  raw: boolean;
  // Largest chunk the watch wants in raw mode.
  window: number;
};

export type InstalledPackage = {
  name: string;
  version: string;
  enabled: boolean;
  kind: 'app' | 'face';
};

export type PackageReply = {
  t: 'pkg';
  ok?: boolean;
  err?: string;
  abi?: AbiInfo;
  pkgs?: InstalledPackage[];
  rx?: number;
  ack?: number;
  got?: number;
  sum?: number;
  name?: string;
  enabled?: boolean;
  removed?: string;
  count?: number;
};

// Package and file names are interpolated into Python source, so they are kept
// to a set that cannot escape a string literal.
const SAFE_NAME = /^[A-Za-z0-9_]+$/;
const SAFE_PATH = /^[A-Za-z0-9_./-]+$/;

export function isPackageReply(value: unknown): value is PackageReply {
  return typeof value === 'object' && value !== null && (value as { t?: string }).t === 'pkg';
}

function checkName(name: string): string {
  if (!SAFE_NAME.test(name)) {
    throw new Error(`Unsafe package name: ${name}`);
  }
  return name;
}

function checkPath(path: string): string {
  if (!SAFE_PATH.test(path) || path.includes('..')) {
    throw new Error(`Unsafe path: ${path}`);
  }
  return path;
}

function line(call: string): string {
  return `${call}\r\n`;
}

export function encodeAbi(): string {
  return line('pkg.abi()');
}

export function encodeLs(): string {
  return line('pkg.ls()');
}

export function encodeReindex(): string {
  return line('pkg.reindex()');
}

export function encodeRecv(path: string, size: number, b64: boolean): string {
  return line(`pkg.recv("${checkPath(path)}", ${size}, ${b64 ? 'True' : 'False'})`);
}

export function encodeRm(name: string): string {
  return line(`pkg.rm("${checkName(name)}")`);
}

export function encodeEnable(name: string): string {
  return line(`pkg.enable("${checkName(name)}")`);
}

export function encodeDisable(name: string): string {
  return line(`pkg.disable("${checkName(name)}")`);
}

// Values cross as a JSON string because Python spells its booleans and its
// null differently, so a JSON literal would not evaluate.
export function encodeCfg(name: string, values: Record<string, unknown>): string {
  const json = JSON.stringify(JSON.stringify(values));
  return line(`pkg.cfg("${checkName(name)}", ${json})`);
}

// The additive checksum the watch returns at the end of a transfer.
export function checksum(data: Uint8Array): number {
  let total = 0;
  for (const byte of data) {
    total = (total + byte) >>> 0;
  }
  return total;
}

export function encodeChunk(chunk: Uint8Array): string {
  return `${bytesToBase64(chunk)}\r\n`;
}

// A package as the phone holds it before install.
export type PackageFile = { path: string; data: Uint8Array };

export type PackageBundle = {
  name: string;
  meta: PackageMeta;
  files: PackageFile[];
};

export type ConfigField = {
  key: string;
  type: 'bool' | 'int' | 'choice';
  label: string;
  default?: unknown;
  min?: number;
  max?: number;
  options?: string[];
};

export type PackageMeta = {
  name: string;
  cls: string;
  label: string;
  version: string;
  kind: 'app' | 'face';
  resident: boolean;
  quick_ring: boolean;
  icon: boolean;
  abi: { mpy: number; arch: number };
  config?: ConfigField[];
};

// True when a package's bytecode will load on this watch.
export function isCompatible(meta: PackageMeta, abi: AbiInfo): boolean {
  if (abi.mpy === null) {
    return false;
  }
  if (meta.abi.mpy !== abi.mpy) {
    return false;
  }
  // arch 0 means plain bytecode, which runs anywhere the version matches.
  return meta.abi.arch === 0 || meta.abi.arch === abi.arch;
}

export function incompatibilityReason(meta: PackageMeta, abi: AbiInfo): string | null {
  if (isCompatible(meta, abi)) {
    return null;
  }
  if (abi.mpy === null) {
    return 'The watch did not report a bytecode version.';
  }
  if (meta.abi.mpy !== abi.mpy) {
    return `Built for bytecode ${meta.abi.mpy}, the watch runs ${abi.mpy}.`;
  }
  return `Built for a different architecture than the watch reports.`;
}
