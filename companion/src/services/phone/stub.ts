import { NO_SOS_PERMISSIONS, type PhoneBridge } from './types';

const unsupported = () => Promise.reject(new Error('The phone bridge is only available in the app, on Android and the desktop'));

// Stands in where there is no native bridge: queries answer false, actions reject, and no events ever arrive.
export const unavailablePhoneBridge: PhoneBridge = {
  available: false,
  kind: 'none',
  supportsCalls: false,
  supportsLocation: false,
  hasNotificationAccess: async () => false,
  openNotificationAccessSettings: unsupported,
  requestCallPermissions: async () => false,
  sosPermissions: async () => NO_SOS_PERMISSIONS,
  requestSosPermissions: async () => NO_SOS_PERMISSIONS,
  openOverlaySettings: unsupported,
  isDoNotDisturb: async () => false,
  subscribe: () => () => {},
  answerCall: unsupported,
  rejectCall: unsupported,
  silenceCall: unsupported,
  mediaCommand: unsupported,
  locate: () => Promise.reject(new Error('This device cannot tell where it is')),
  associateWatch: async () => false,
  isIgnoringBatteryOptimizations: async () => false,
  requestIgnoreBatteryOptimizations: async () => false,
};
