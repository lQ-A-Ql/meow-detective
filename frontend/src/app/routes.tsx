import { createBrowserRouter, type RouteObject } from 'react-router';
import { useTranslation } from 'react-i18next';
import { LoaderCircle } from '@/components/icons';
import { Layout } from '@/components/layout/Layout';
import { RouteErrorBoundary } from '@/app/components/RouteErrorBoundary';
import { isDevOrAuditMode } from '@/lib/env';

/**
 * Painted inside the layout shell while the first lazy page module resolves.
 * React Router only renders a route-level `HydrateFallback` for the initial load of a
 * lazy route (`state.renderFallback`); later client navigations keep the current page
 * painted until the next route module arrives, so they need no separate fallback.
 */
function RouteLoadingFallback() {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      aria-live="polite"
      aria-label={t('common.loading')}
      className="grid min-h-40 place-items-center gap-2 p-6 text-forensics-muted"
    >
      <LoaderCircle size={18} className="animate-spin" aria-hidden="true" />
      <span className="text-[12px]">{t('common.loading')}</span>
    </div>
  );
}

export const appRoutes: RouteObject[] = [
  {
    path: '/',
    Component: Layout,
    children: [
      {
        // Pathless boundary: a page crash keeps the app shell and stays inside the page area.
        errorElement: <RouteErrorBoundary />,
        HydrateFallback: RouteLoadingFallback,
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
            path: 'infrastructure/linux',
            lazy: async () => ({
              Component: (await import('./pages/LinuxInfrastructure')).LinuxInfrastructure,
            }),
          },
          {
            path: 'emulation',
            lazy: async () => ({
              Component: (await import('./pages/Emulation')).Emulation,
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
        ],
      },
    ],
  },
];

export const router = createBrowserRouter(appRoutes);
