import type { PhoneBridge } from './types';

const unsupported = () => Promise.reject(new Error('The phone bridge is only available on Android'));

// Stands in where there is no native bridge: queries answer false, actions reject, and no events ever arrive.
export const unavailablePhoneBridge: PhoneBridge = {
  available: false,
  hasNotificationAccess: async () => false,
  openNotificationAccessSettings: unsupported,
  requestCallPermissions: async () => false,
  isDoNotDisturb: async () => false,
  subscribe: () => () => {},
  answerCall: unsupported,
  rejectCall: unsupported,
  mediaCommand: unsupported,
  startKeepAlive: unsupported,
  stopKeepAlive: async () => {},
};
