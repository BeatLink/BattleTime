// Types for the encoder inside the qrcode package, which the companion imports on its own.

declare module 'qrcode/lib/core/qrcode' {
  export type ErrorCorrectionLevelName = 'L' | 'M' | 'Q' | 'H';

  // The grid of modules, row by row, one byte a module, 1 for dark.
  export type QrModules = { size: number; data: Uint8Array };

  export type QrSymbol = { modules: QrModules; version: number };

  export function create(text: string, options?: { errorCorrectionLevel?: ErrorCorrectionLevelName }): QrSymbol;
}
