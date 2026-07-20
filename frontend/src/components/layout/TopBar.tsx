import { useTranslation } from 'react-i18next';
import { NavLink, useNavigate } from 'react-router';
import { Search, Activity, Settings } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { HorizontalScroll } from '@/components/layout/HorizontalScroll';
import { useCurrentCase } from '@/features/case/hooks';
import { isDevOrAuditMode } from '@/lib/env';
import { useJobsSnapshot } from '@/features/jobs/hooks';
import { useUiStore } from '@/stores/ui-store';

const pageKeys = [
  { to: '/', page: 'home' as const },
  { to: '/files', page: 'files' as const },
  { to: '/analysis', page: 'analysis' as const },
  ...(isDevOrAuditMode() ? [{ to: '/v2', page: 'v2' as const }] : []),
  { to: '/v3', page: 'v3' as const },
  { to: '/search', page: 'search' as const },
  { to: '/timeline', page: 'timeline' as const },
  { to: '/artifacts', page: 'artifacts' as const },
  { to: '/reports', page: 'reports' as const },
];

export function TopBar() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { data: currentCase } = useCurrentCase();
  const { data: jobs } = useJobsSnapshot();
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const setCurrentPage = useUiStore((state) => state.setCurrentPage);
  const globalSearchQuery = useUiStore((state) => state.globalSearchQuery);
  const setGlobalSearchQuery = useUiStore((state) => state.setGlobalSearchQuery);

  const runningCount = jobs?.filter((job) => job.status === 'running').length ?? 0;

  return (
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel px-6 py-3 text-xs">
      <div className="flex items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-6">
          <HorizontalScroll className="flex min-w-0 items-center gap-5">
            {pageKeys.map((link) => (
              <NavLink
                key={link.to}
                to={link.to}
                onClick={() => setCurrentPage(link.page)}
                className={({ isActive }) =>
                  `whitespace-nowrap underline-offset-4 transition-colors hover:text-forensics-text hover:decoration-forensics-sakura-400 ${
                    isActive
                      ? 'font-light text-forensics-text underline decoration-forensics-sakura-500 decoration-1'
                      : 'text-forensics-muted'
                  }`
                }
              >
                {t(`topBar.links.${link.page}.label`)}
              </NavLink>
            ))}
          </HorizontalScroll>
          <div className="hidden 2xl:block min-w-0 border-l border-forensics-border pl-4 text-[11px] text-forensics-muted">
            <span className="block max-w-48 truncate">{currentCase?.name ?? t('topBar.case.noCase')}</span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-3 text-forensics-muted">
          <div className="flex items-center gap-2 border border-forensics-border bg-transparent px-2 py-1 rounded-none">
            <Search size={12} className="text-forensics-muted-light" />
            <Input
              value={globalSearchQuery}
              onChange={(event) => setGlobalSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && globalSearchQuery.trim()) {
                  navigate(`/search?q=${encodeURIComponent(globalSearchQuery.trim())}`);
                }
              }}
              variant="search"
              inputSize="inline"
              className="w-40 xl:w-56 text-xs font-mono"
              placeholder={t('topBar.search.placeholder')}
            />
          </div>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            onClick={toggleDrawer}
            title={t('topBar.jobs.running', { count: runningCount })}
            aria-label={t('topBar.jobs.running', { count: runningCount })}
            className="relative border border-transparent text-forensics-text-tertiary hover:border-forensics-border hover:bg-forensics-surface hover:text-forensics-text"
          >
            <Activity size={13} />
            {runningCount > 0 ? (
              <span className="absolute -right-1 -top-1 min-w-3 border border-forensics-panel bg-forensics-primary-blue px-0.5 text-center text-[9px] text-white">
                {runningCount}
              </span>
            ) : null}
          </Button>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            onClick={() => navigate('/settings')}
            title={t('settings.title')}
            aria-label={t('settings.title')}
            className="text-forensics-text-tertiary hover:bg-forensics-surface hover:text-forensics-text"
          >
            <Settings size={13} />
          </Button>
        </div>
      </div>
    </div>
  );
}
