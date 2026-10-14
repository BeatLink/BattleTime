import type { ComponentType, JSX } from 'preact';

import { useWatch } from '@/hooks/use-watch';
import { RouterProvider, useRouter } from '@/navigation';
import AlarmsScreen from '@/screens/alarms';
import AppsScreen from '@/screens/apps';
import BackupScreen from '@/screens/backup';
import ConsoleScreen from '@/screens/console';
import FilesScreen from '@/screens/files';
import FirmwareScreen from '@/screens/firmware';
import LauncherScreen from '@/screens/launcher';
import NotesScreen from '@/screens/notes';
import NotificationsScreen from '@/screens/notifications';
import PluginsScreen from '@/screens/plugins';
import QrCodeScreen from '@/screens/qr-code';
import QuickAppsScreen from '@/screens/quick-apps';
import RemoteScreen from '@/screens/remote';
import ScanScreen from '@/screens/scan';
import SettingsScreen from '@/screens/settings';
import SetupScreen from '@/screens/setup';
import StepsScreen from '@/screens/steps';
import StoreScreen from '@/screens/store';
import WatchScreen from '@/screens/watch';
import WatchSettingsScreen from '@/screens/watch-settings';
import { isTabRoute, STACK_TITLES, type Route, type TabRoute } from '@/state/routes';
import { WatchProvider } from '@/state/watch-provider';
import { isTabVisible, type TabName } from '@/state/visible-tabs';

// ============================================================================
// Screens
// ============================================================================

const SCREENS: Record<Route, ComponentType> = {
  '/': WatchScreen,
  '/firmware': FirmwareScreen,
  '/notifications': NotificationsScreen,
  '/console': ConsoleScreen,
  '/settings': SettingsScreen,
  '/scan': ScanScreen,
  '/steps': StepsScreen,
  '/alarms': AlarmsScreen,
  '/watch-settings': WatchSettingsScreen,
  '/files': FilesScreen,
  '/notes': NotesScreen,
  '/qr-code': QrCodeScreen,
  '/remote': RemoteScreen,
  '/quick-apps': QuickAppsScreen,
  '/backup': BackupScreen,
  '/apps': AppsScreen,
  '/store': StoreScreen,
  '/plugins': PluginsScreen,
  '/launcher': LauncherScreen,
  '/setup': SetupScreen,
};

// ============================================================================
// The tab bar
// ============================================================================

// Outline icons in the style of the ones the phone app had, drawn in the current colour.
const TAB_ICONS: Record<TabName, JSX.Element> = {
  index: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round">
      <rect x="6" y="6" width="12" height="12" rx="3" />
      <path d="M9 6l1-3h4l1 3M9 18l1 3h4l1-3M12 9v3l2 1" stroke-linecap="round" />
    </svg>
  ),
  firmware: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M7 17H6a4 4 0 0 1-.5-8A6 6 0 0 1 17 8a4.5 4.5 0 0 1 1 9h-1" />
      <path d="M12 11v9M9 17l3 3 3-3" />
    </svg>
  ),
  notifications: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15L6 16z" />
      <path d="M10 20a2 2 0 0 0 4 0" />
    </svg>
  ),
  console: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M7 9l3 3-3 3M12 15h5" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" />
    </svg>
  ),
};

const TABS: { route: TabRoute; name: TabName; title: string }[] = [
  { route: '/', name: 'index', title: 'Watch' },
  { route: '/firmware', name: 'firmware', title: 'Firmware' },
  { route: '/notifications', name: 'notifications', title: 'Notifications' },
  { route: '/console', name: 'console', title: 'Console' },
  { route: '/settings', name: 'settings', title: 'Settings' },
];

// A tab whose screen has nothing to do is left out of the bar rather than shown as something that does not work.
function TabBar({ route }: { route: TabRoute }) {
  const { push } = useRouter();
  const { connection, watchMode, capabilities } = useWatch();
  return (
    <nav class="tab-bar">
      {TABS.filter((tab) => isTabVisible(tab.name, connection, watchMode, capabilities)).map((tab) => (
        <button
          type="button"
          key={tab.route}
          class={tab.route === route ? 'tab active' : 'tab'}
          aria-current={tab.route === route ? 'page' : undefined}
          onClick={() => push(tab.route)}>
          {TAB_ICONS[tab.name]}
          {tab.title}
        </button>
      ))}
    </nav>
  );
}

// ============================================================================
// The window
// ============================================================================

// The tabs with their bar, or a screen opened over them with a bar to go back.
function Shell() {
  const { route, back } = useRouter();
  const Screen = SCREENS[route];
  if (isTabRoute(route)) {
    return (
      <div class="app with-tabs">
        <div class="content">
          <Screen key={route} />
        </div>
        <TabBar route={route} />
      </div>
    );
  }
  return (
    <div class="app with-header">
      <header class="header">
        <button type="button" class="back-button" aria-label="Back" onClick={back}>
          ‹ Back
        </button>
        <span class="header-title">{STACK_TITLES[route]}</span>
      </header>
      <div class="content">
        <Screen key={route} />
      </div>
    </div>
  );
}

export function App() {
  return (
    <RouterProvider>
      <WatchProvider>
        <Shell />
      </WatchProvider>
    </RouterProvider>
  );
}
