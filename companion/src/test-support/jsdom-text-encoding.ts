// Gives jsdom tests Node's TextEncoder and TextDecoder, which WebKitGTK and Android's WebView have but jsdom lacks.
// Import it before anything that encodes text when the module loads.

import { TextDecoder, TextEncoder } from 'util';

Object.assign(globalThis, { TextEncoder, TextDecoder });
