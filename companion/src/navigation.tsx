import { createContext, type ComponentChildren } from 'preact';
import { useCallback, useContext, useEffect, useMemo, useState } from 'preact/hooks';

import { hashForRoute, routeFromHash, type Route } from '@/state/routes';

type Router = {
  route: Route;
  // Opens a screen, which the back button then leaves.
  push: (route: Route) => void;
  // Goes back to the screen before, or to the Watch tab when there is none.
  back: () => void;
};

const RouterContext = createContext<Router | null>(null);

// Follows the address hash, so the web view's history is the stack of screens.
export function RouterProvider({ children }: { children: ComponentChildren }) {
  const [route, setRoute] = useState<Route>(() => routeFromHash(window.location.hash));

  useEffect(() => {
    const follow = () => setRoute(routeFromHash(window.location.hash));
    window.addEventListener('hashchange', follow);
    return () => window.removeEventListener('hashchange', follow);
  }, []);

  const push = useCallback((next: Route) => {
    if (routeFromHash(window.location.hash) !== next) {
      window.location.hash = hashForRoute(next);
    }
  }, []);

  const back = useCallback(() => {
    if (window.history.length > 1) {
      window.history.back();
    } else {
      window.location.hash = hashForRoute('/');
    }
  }, []);

  const router = useMemo(() => ({ route, push, back }), [route, push, back]);
  return <RouterContext.Provider value={router}>{children}</RouterContext.Provider>;
}

export function useRouter(): Router {
  const router = useContext(RouterContext);
  if (!router) {
    throw new Error('useRouter must be used inside RouterProvider');
  }
  return router;
}
