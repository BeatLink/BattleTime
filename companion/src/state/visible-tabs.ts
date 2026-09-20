// Which screens have something to offer, given what is on the other end of
// the connection.
//
// A screen that cannot do its job is left out of the tab bar rather than
// shown as something that does not work.

import type { ConnectionState, WatchMode } from '@/ble/transport';

export type TabName =
  | 'index'
  | 'packages'
  | 'firmware'
  | 'notifications'
  | 'console'
  | 'settings';

// The connection screen and the app's own settings never depend on a watch.
const ALWAYS: TabName[] = ['index', 'settings'];

// These need a watch that is running its firmware: they speak Gadgetbridge,
// the package manager or the REPL, and a bootloader answers none of them.
const NEEDS_FIRMWARE: TabName[] = ['packages', 'notifications', 'console'];

export function visibleTabs(connection: ConnectionState, mode: WatchMode): TabName[] {
  if (connection !== 'connected') {
    return [...ALWAYS];
  }
  // A firmware update is the one thing a bootloader is good for.
  if (mode === 'bootloader') {
    return [...ALWAYS, 'firmware'];
  }
  return [...ALWAYS, 'firmware', ...NEEDS_FIRMWARE];
}

export function isTabVisible(
  tab: TabName,
  connection: ConnectionState,
  mode: WatchMode,
): boolean {
  return visibleTabs(connection, mode).includes(tab);
}
