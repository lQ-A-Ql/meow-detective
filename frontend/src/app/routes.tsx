import { createBrowserRouter } from 'react-router';
import { Layout } from '@/components/layout/Layout';

export const router = createBrowserRouter([
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
]);
