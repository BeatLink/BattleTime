// iOS and web have no phone bridge; Android gets the native one from index.android.ts.
import { unavailablePhoneBridge } from './stub';
import type { PhoneBridge } from './types';

export type { MediaCommand, PhoneBridge, PhoneEvent } from './types';

export const phoneBridge: PhoneBridge = unavailablePhoneBridge;
