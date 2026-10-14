// Android intents for remote buttons: the Tauri app's phone plugin here, the Expo app's module in android-intents.android.ts.
import { tauriIntents, tauriPhoneDevice } from './phone/tauri-android';
import type { AndroidIntents } from './remote-actions';

export const androidIntents: AndroidIntents | null = tauriIntents(tauriPhoneDevice());
