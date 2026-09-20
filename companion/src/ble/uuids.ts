// Nordic UART Service, which wasp-os uses for its REPL and the Gadgetbridge protocol.
export const NUS_SERVICE = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';

// The phone writes to RX and the watch notifies on TX.
export const NUS_RX = '6e400002-b5a3-f393-e0a9-e50e24dcca9e';
export const NUS_TX = '6e400003-b5a3-f393-e0a9-e50e24dcca9e';

// Attribute bytes reserved by the ATT header on every write.
export const ATT_OVERHEAD = 3;

// The default BLE MTU when the watch does not negotiate a larger one.
export const DEFAULT_MTU = 23;
