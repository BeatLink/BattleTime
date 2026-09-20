import { describe, expect, it } from '@jest/globals';

import {
  checksum,
  encodeCfg,
  encodeChunk,
  encodeDisable,
  encodeEnable,
  encodeRecv,
  encodeRm,
  incompatibilityReason,
  isCompatible,
  isPackageReply,
  type AbiInfo,
  type PackageMeta,
} from './packages';

const abi: AbiInfo = { mpy: 6, arch: 0, raw: false, window: 96 };

const meta: PackageMeta = {
  name: 'calculator',
  cls: 'CalculatorApp',
  label: 'Calculator',
  version: '1.0.0',
  kind: 'app',
  resident: false,
  quick_ring: false,
  icon: true,
  abi: { mpy: 6, arch: 0 },
};

describe('command encoding', () => {
  it('ends every command with a newline the REPL accepts', () => {
    expect(encodeRm('calculator')).toBe('pkg.rm("calculator")\r\n');
    expect(encodeEnable('calculator')).toBe('pkg.enable("calculator")\r\n');
    expect(encodeDisable('calculator')).toBe('pkg.disable("calculator")\r\n');
  });

  it('spells the receive flag the way Python does', () => {
    expect(encodeRecv('pkg/calculator/app.mpy', 1631, true)).toBe(
      'pkg.recv("pkg/calculator/app.mpy", 1631, True)\r\n',
    );
    expect(encodeRecv('pkg/a/b', 1, false)).toContain('False');
  });

  it('sends config as a JSON string rather than a literal', () => {
    // A bare JSON literal would be a NameError on the watch, since Python
    // writes True rather than true.
    const encoded = encodeCfg('calculator', { loud: true, steps: 3 });
    expect(encoded).toBe('pkg.cfg("calculator", "{\\"loud\\":true,\\"steps\\":3}")\r\n');
    expect(encoded).not.toContain('{loud');
  });

  it('refuses a name that could escape the Python string', () => {
    expect(() => encodeRm('calc"); import os; os.remove("x')).toThrow(/Unsafe/);
    expect(() => encodeEnable('has space')).toThrow(/Unsafe/);
    expect(() => encodeRecv('../../etc/passwd', 1, true)).toThrow(/Unsafe/);
  });

  it('allows the paths a package actually uses', () => {
    expect(() => encodeRecv('pkg/music_player/icon.rle', 358, true)).not.toThrow();
  });
});

describe('chunk encoding', () => {
  it('base64 encodes a chunk and terminates the line', () => {
    expect(encodeChunk(new Uint8Array([104, 105]))).toBe('aGk=\r\n');
  });

  it('round trips every byte value', () => {
    const data = new Uint8Array(256);
    for (let i = 0; i < 256; i++) data[i] = i;
    const encoded = encodeChunk(data).trim();
    const decoded = Buffer.from(encoded, 'base64');
    expect(Uint8Array.from(decoded)).toEqual(data);
  });
});

describe('checksum', () => {
  it('sums the bytes the way the watch does', () => {
    expect(checksum(new Uint8Array([1, 2, 3]))).toBe(6);
    expect(checksum(new Uint8Array())).toBe(0);
  });

  it('stays a 32 bit unsigned value', () => {
    const big = new Uint8Array(200000).fill(255);
    const result = checksum(big);
    expect(result).toBeGreaterThanOrEqual(0);
    expect(result).toBe((200000 * 255) >>> 0);
  });
});

describe('compatibility', () => {
  it('accepts a package built for the same bytecode version', () => {
    expect(isCompatible(meta, abi)).toBe(true);
    expect(incompatibilityReason(meta, abi)).toBeNull();
  });

  it('rejects a package built for another bytecode version', () => {
    const older = { ...meta, abi: { mpy: 5, arch: 0 } };
    expect(isCompatible(older, abi)).toBe(false);
    expect(incompatibilityReason(older, abi)).toContain('bytecode 5');
  });

  it('rejects when the watch reports no version', () => {
    const unknown: AbiInfo = { ...abi, mpy: null, arch: null };
    expect(isCompatible(meta, unknown)).toBe(false);
    expect(incompatibilityReason(meta, unknown)).toContain('did not report');
  });

  it('accepts plain bytecode whatever the architecture', () => {
    const otherArch: AbiInfo = { ...abi, arch: 5 };
    expect(isCompatible(meta, otherArch)).toBe(true);
  });

  it('rejects native code built for another architecture', () => {
    const native = { ...meta, abi: { mpy: 6, arch: 3 } };
    expect(isCompatible(native, { ...abi, arch: 5 })).toBe(false);
  });
});

describe('reply recognition', () => {
  it('spots a package reply', () => {
    expect(isPackageReply({ t: 'pkg', ok: true })).toBe(true);
  });

  it('ignores other traffic', () => {
    expect(isPackageReply({ t: 'music', n: 'play' })).toBe(false);
    expect(isPackageReply(null)).toBe(false);
    expect(isPackageReply('pkg')).toBe(false);
  });
});
