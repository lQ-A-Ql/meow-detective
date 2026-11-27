import { useNavigate } from 'react-router';
import { useCurrentCase } from '@/features/case/hooks';
import { useJobsSnapshot } from '@/features/jobs/hooks';
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
  { to: '/logs', page: 'logs' },
];

/** Navigation destinations and job status for the activity rail. */
export function useNavigationModel() {
  const navigate = useNavigate();
  const currentCase = useCurrentCase();
  const { data: jobs } = useJobsSnapshot(currentCase.data?.id);
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);

  return {
    links: productionLinks,
    runningCount: jobs?.filter((job) => job.status === 'running').length ?? 0,
    toggleDrawer,
    openSettings() {
      navigate('/settings');
    },
  };
}

export type NavigationModel = ReturnType<typeof useNavigationModel>;
