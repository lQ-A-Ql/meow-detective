import { createBrowserRouter, type RouteObject } from 'react-router';
import { Layout } from '@/components/layout/Layout';
import { isDevOrAuditMode } from '@/lib/env';

export const appRoutes: RouteObject[] = [
  {
    path: '/',
    Component: Layout,
    children: [
      {
        index: true,
        lazy: async () => ({
          Component: (await import('./pages/CaseHome')).CaseHome,
        }),
      },
      {
        path: 'analysis',
        lazy: async () => ({
          Component: (await import('./pages/DataAnalysis')).DataAnalysis,
        }),
      },
      ...(isDevOrAuditMode()
        ? [
            {
              path: 'v2',
              lazy: async () => ({
                Component: (await import('./pages/V2Workbench')).V2Workbench,
              }),
            },
          ]
        : []),
      {
        path: 'v3',
        lazy: async () => ({
          Component: (await import('./pages/V3Dashboard')).V3Dashboard,
        }),
      },
      {
        path: 'files',
        lazy: async () => ({
          Component: (await import('./pages/FileBrowser')).FileBrowser,
        }),
      },
      {
        path: 'search',
        lazy: async () => ({
          Component: (await import('./pages/Search')).Search,
        }),
      },
      {
        path: 'timeline',
        lazy: async () => ({
          Component: (await import('./pages/Timeline')).Timeline,
        }),
      },
      {
        path: 'artifacts',
        lazy: async () => ({
          Component: (await import('./pages/Artifacts')).Artifacts,
        }),
      },
      {
        path: 'reports',
        lazy: async () => ({
          Component: (await import('./pages/Reports')).Reports,
        }),
      },
      {
        path: 'settings',
        lazy: async () => ({
          Component: (await import('./pages/Settings')).Settings,
        }),
      },
    ],
  },
];

export const router = createBrowserRouter(appRoutes);
