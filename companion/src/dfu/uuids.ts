// Nordic legacy DFU, which BattleTime's firmware and wasp-os's bootloader both speak.
export const LEGACY_DFU_SERVICE = '00001530-1212-efde-1523-785feabcd123';
export const LEGACY_CONTROL_POINT = '00001531-1212-efde-1523-785feabcd123';
export const LEGACY_PACKET = '00001532-1212-efde-1523-785feabcd123';
export const LEGACY_VERSION = '00001534-1212-efde-1523-785feabcd123';

// Every DFU write is a fixed 20 bytes, the payload every connection supports,
// because a bootloader does not negotiate a larger MTU.
export const DFU_PAYLOAD_SIZE = 20;
