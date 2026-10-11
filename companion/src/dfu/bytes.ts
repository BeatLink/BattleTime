// Little-endian helpers and Bluetooth address arithmetic for the DFU controller.

export function u16le(value: number): Uint8Array {
  return Uint8Array.from([value & 0xff, (value >> 8) & 0xff]);
}

export function u32le(value: number): Uint8Array {
  return Uint8Array.from([
    value & 0xff,
    (value >>> 8) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 24) & 0xff,
  ]);
}

export function readU32le(bytes: Uint8Array, offset: number): number {
  return (
    (bytes[offset] |
      (bytes[offset + 1] << 8) |
      (bytes[offset + 2] << 16) |
      (bytes[offset + 3] << 24)) >>>
    0
  );
}

export function concat(...parts: (Uint8Array | number[])[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part instanceof Uint8Array ? part : Uint8Array.from(part), offset);
    offset += part.length;
  }
  return out;
}

// wasp-os's bootloader answers at the watch address plus one.
export function nextAddress(address: string): string {
  const parts = address.split(':');
  if (parts.length !== 6 || parts.some((part) => !/^[0-9a-fA-F]{2}$/.test(part))) {
    throw new Error(`Not a Bluetooth address: ${address}`);
  }
  const bytes = parts.map((part) => parseInt(part, 16));
  for (let i = bytes.length - 1; i >= 0; i--) {
    bytes[i] = (bytes[i] + 1) & 0xff;
    if (bytes[i] !== 0) {
      break;
    }
  }
  return bytes.map((byte) => byte.toString(16).padStart(2, '0').toUpperCase()).join(':');
}
