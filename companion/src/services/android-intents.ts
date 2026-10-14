// Android intents for remote buttons, started by the app's phone plugin.
import { tauriIntents, tauriPhoneDevice } from './phone/tauri-android';
import type { AndroidIntents } from './remote-actions';

export const androidIntents: AndroidIntents | null = tauriIntents(tauriPhoneDevice());
