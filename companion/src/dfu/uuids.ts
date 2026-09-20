// The two DFU flavours an nRF52 bootloader can speak, plus the buttonless
// characteristic an application exposes to jump into the secure bootloader.

// Nordic legacy DFU, which the bootloader the PineTime ships with speaks.
export const LEGACY_DFU_SERVICE = '00001530-1212-efde-1523-785feabcd123';
export const LEGACY_CONTROL_POINT = '00001531-1212-efde-1523-785feabcd123';
export const LEGACY_PACKET = '00001532-1212-efde-1523-785feabcd123';
export const LEGACY_VERSION = '00001534-1212-efde-1523-785feabcd123';

// Nordic secure DFU, from SDK 12 onwards. Both the bootloader and the
// buttonless jump in the application live under the same 16-bit service.
export const SECURE_DFU_SERVICE = '0000fe59-0000-1000-8000-00805f9b34fb';
export const SECURE_CONTROL_POINT = '8ec90001-f315-4f60-9fb8-838830daea50';
export const SECURE_PACKET = '8ec90002-f315-4f60-9fb8-838830daea50';
export const SECURE_BUTTONLESS = '8e400001-f315-4f60-9fb8-838830daea50';

// Every DFU write is a fixed 20 bytes, the payload every connection supports,
// because the bootloader does not negotiate a larger MTU.
export const DFU_PAYLOAD_SIZE = 20;
