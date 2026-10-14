// The window's screens and where each sits: the five tabs, and the screens opened over them.
//
// The route lives in the address's hash, so the web view's history, and the Android back button, step back through it.

export type TabRoute = '/' | '/firmware' | '/notifications' | '/console' | '/settings';

export type StackRoute =
  | '/scan'
  | '/steps'
  | '/alarms'
  | '/watch-settings'
  | '/files'
  | '/notes'
  | '/qr-code'
  | '/remote'
  | '/quick-apps'
  | '/backup'
  | '/apps'
  | '/store'
  | '/plugins'
  | '/launcher'
  | '/setup';

export type Route = TabRoute | StackRoute;

export const TAB_ROUTES: readonly TabRoute[] = ['/', '/firmware', '/notifications', '/console', '/settings'];

// The title in the bar above each screen opened over the tabs.
export const STACK_TITLES: Record<StackRoute, string> = {
  '/scan': 'Find a watch',
  '/steps': 'Steps',
  '/alarms': 'Alarms',
  '/watch-settings': 'Watch settings',
  '/files': 'Files',
  '/notes': 'Notes',
  '/qr-code': 'QR code',
  '/remote': 'Remote',
  '/quick-apps': 'Quick apps',
  '/backup': 'Backup',
  '/apps': 'Apps',
  '/store': 'Store',
  '/plugins': 'Plugins',
  '/launcher': 'Launcher',
  '/setup': 'Set up the watch',
};

export function isTabRoute(route: Route): route is TabRoute {
  return (TAB_ROUTES as readonly string[]).includes(route);
}

// The route an address hash names; anything unknown is the Watch tab.
export function routeFromHash(hash: string): Route {
  const path = hash.replace(/^#/, '') || '/';
  return isTabRoute(path as Route) || path in STACK_TITLES ? (path as Route) : '/';
}

// The address hash for a route.
export function hashForRoute(route: Route): string {
  return `#${route}`;
}
