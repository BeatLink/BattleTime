import type { BridgeKind } from './types';

// What the screens call the device the app runs on.
export function deviceWord(kind: BridgeKind): 'computer' | 'phone' {
  return kind === 'computer' ? 'computer' : 'phone';
}
