import { useNavigate } from 'react-router';
import { useCurrentCase } from '@/features/case/hooks';
import { useJobsSnapshot } from '@/features/jobs/hooks';
import { isDevOrAuditMode } from '@/lib/env';
import { useUiStore, type PageKey } from '@/stores/ui-store';

export interface TopBarLink {
  to: string;
  page: PageKey;
}

const productionLinks: TopBarLink[] = [
  { to: '/', page: 'home' },
  { to: '/files', page: 'files' },
  { to: '/emulation', page: 'emulation' },
  { to: '/analysis', page: 'analysis' },
  { to: '/v3', page: 'v3' },
  { to: '/search', page: 'search' },
  { to: '/timeline', page: 'timeline' },
  { to: '/artifacts', page: 'artifacts' },
  { to: '/reports', page: 'reports' },
];

export function useTopBarModel() {
  const navigate = useNavigate();
  const { data: currentCase } = useCurrentCase();
  const { data: jobs } = useJobsSnapshot();
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const setCurrentPage = useUiStore((state) => state.setCurrentPage);
  const globalSearchQuery = useUiStore((state) => state.globalSearchQuery);
  const setGlobalSearchQuery = useUiStore((state) => state.setGlobalSearchQuery);
  const links = isDevOrAuditMode()
    ? [...productionLinks.slice(0, 3), { to: '/v2', page: 'v2' as const }, ...productionLinks.slice(3)]
    : productionLinks;

  return {
    links,
    currentCaseName: currentCase?.name,
    runningCount: jobs?.filter((job) => job.status === 'running').length ?? 0,
    globalSearchQuery,
    setGlobalSearchQuery,
    selectPage: setCurrentPage,
    toggleDrawer,
    submitSearch() {
      const query = globalSearchQuery.trim();
      if (query) navigate(`/search?q=${encodeURIComponent(query)}`);
    },
    openSettings() {
      navigate('/settings');
    },
  };
}

export type TopBarModel = ReturnType<typeof useTopBarModel>;
