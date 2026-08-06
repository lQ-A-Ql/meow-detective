import { describe, expect, it } from 'vitest';
import { appRoutes } from './routes';

describe('app route code splitting', () => {
  it('keeps page routes lazy-loaded while preserving the static layout shell', () => {
    const rootRoute = appRoutes[0];

    expect(rootRoute.Component).toBeDefined();
    expect(rootRoute.lazy).toBeUndefined();
    expect(rootRoute.children).toBeDefined();

    for (const route of rootRoute.children ?? []) {
      expect(route.lazy).toEqual(expect.any(Function));
      expect(route.Component).toBeUndefined();
    }
  });

  it('includes the V2 governance workbench route', () => {
    const rootRoute = appRoutes[0];
    const v2Route = rootRoute.children?.find((route) => route.path === 'v2');

    expect(v2Route).toBeDefined();
    expect(v2Route?.lazy).toEqual(expect.any(Function));
  });

  it('exposes the image emulation workspace as a lazy route', () => {
    const rootRoute = appRoutes[0];
    const emulationRoute = rootRoute.children?.find((route) => route.path === 'emulation');

    expect(emulationRoute).toBeDefined();
    expect(emulationRoute?.lazy).toEqual(expect.any(Function));
  });
});
