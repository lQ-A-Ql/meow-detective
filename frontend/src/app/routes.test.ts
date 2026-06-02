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
});
