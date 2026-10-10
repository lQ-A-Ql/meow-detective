import { describe, expect, it } from 'vitest';
import { appRoutes } from '@/app/routes';

/** Page routes live under the pathless route-level error boundary inside the layout route. */
function boundaryRoute() {
  return appRoutes[0]?.children?.[0];
}

function pageRoutes() {
  return boundaryRoute()?.children ?? [];
}

describe('app route code splitting', () => {
  it('keeps page routes lazy-loaded while preserving the static layout shell', () => {
    const rootRoute = appRoutes[0];

    expect(rootRoute.Component).toBeDefined();
    expect(rootRoute.lazy).toBeUndefined();
    expect(rootRoute.children).toBeDefined();
    expect(pageRoutes().length).toBeGreaterThan(0);

    for (const route of pageRoutes()) {
      expect(route.lazy).toEqual(expect.any(Function));
      expect(route.Component).toBeUndefined();
    }
  });

  it('contains a page crash in a route-level error boundary under the layout shell', () => {
    const boundary = boundaryRoute();

    // Pathless, so the shell (sidebar/top bar) survives a page rendering failure.
    expect(boundary?.path).toBeUndefined();
    expect(boundary?.errorElement).toBeDefined();
    expect(pageRoutes().some((route) => route.index === true)).toBe(true);
  });

  it('includes the V2 governance workbench route', () => {
    const v2Route = pageRoutes().find((route) => route.path === 'v2');

    expect(v2Route).toBeDefined();
    expect(v2Route?.lazy).toEqual(expect.any(Function));
  });

  it('exposes the image emulation workspace as a lazy route', () => {
    const emulationRoute = pageRoutes().find((route) => route.path === 'emulation');

    expect(emulationRoute).toBeDefined();
    expect(emulationRoute?.lazy).toEqual(expect.any(Function));
  });
});
