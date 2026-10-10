import { useNavigate } from 'react-router';
import { useJobsSnapshot } from '@/features/jobs/hooks';
import { isDevOrAuditMode } from '@/lib/env';
import { useUiStore, type PageKey } from '@/stores/ui-store';

export interface NavigationLink {
  to: string;
  page: PageKey;
}

const productionLinks: NavigationLink[] = [
  { to: '/', page: 'home' },
  { to: '/files', page: 'files' },
  { to: '/emulation', page: 'emulation' },
  { to: '/analysis', page: 'analysis' },
  { to: '/infrastructure/linux', page: 'infrastructure' },
  { to: '/v3', page: 'v3' },
  { to: '/search', page: 'search' },
  { to: '/timeline', page: 'timeline' },
  { to: '/artifacts', page: 'artifacts' },
  { to: '/reports', page: 'reports' },
];

/** Navigation destinations and job status for the activity rail. */
export function useNavigationModel() {
  const navigate = useNavigate();
  const { data: jobs } = useJobsSnapshot();
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const setCurrentPage = useUiStore((state) => state.setCurrentPage);
  const links = isDevOrAuditMode()
    ? [...productionLinks.slice(0, 3), { to: '/v2', page: 'v2' as const }, ...productionLinks.slice(3)]
    : productionLinks;

  return {
    links,
    runningCount: jobs?.filter((job) => job.status === 'running').length ?? 0,
    selectPage: setCurrentPage,
    toggleDrawer,
    openSettings() {
      navigate('/settings');
    },
  };
}

export type NavigationModel = ReturnType<typeof useNavigationModel>;
