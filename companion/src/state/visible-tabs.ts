// Which screens have something to offer, given what is on the other end of
// the connection.
//
// A screen that cannot do its job is left out of the tab bar rather than
// shown as something that does not work.

import type { ConnectionState, WatchMode } from '@/ble/transport';
import type { WatchCapabilities } from '@/protocol/neotime';

export type TabName =
  | 'index'
  | 'firmware'
  | 'notifications'
  | 'console'
  | 'settings';

// The connection screen and the app's own settings never depend on a watch.
const ALWAYS: TabName[] = ['index', 'settings'];

// These speak messages over the UART channel, which a bootloader and an early NeoTime lack.
const NEEDS_UART: TabName[] = ['notifications', 'console'];

export function visibleTabs(
  connection: ConnectionState,
  mode: WatchMode,
  capabilities: WatchCapabilities,
): TabName[] {
  if (connection !== 'connected') {
    return [...ALWAYS];
  }
  // A firmware update is the one thing a bootloader is good for.
  if (mode === 'bootloader') {
    return [...ALWAYS, 'firmware'];
  }
  return [...ALWAYS, 'firmware', ...(capabilities.uart ? NEEDS_UART : [])];
}

export function isTabVisible(
  tab: TabName,
  connection: ConnectionState,
  mode: WatchMode,
  capabilities: WatchCapabilities,
): boolean {
  return visibleTabs(connection, mode, capabilities).includes(tab);
}
