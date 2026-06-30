import { useTranslation } from 'react-i18next';
import { NavLink, useLocation, useNavigate } from 'react-router';
import { Search, Activity, Settings, AlertTriangle } from 'lucide-react';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { isDevOrAuditMode } from '@/lib/env';
import {
  deriveEvidenceHashStatus,
  getCacheStateLabel,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  getFreshnessLabel,
  getImportPhaseLabel,
  getImportPhaseStateLabel,
  getPartialKindLabel,
  useImportEventState,
} from '@/features/jobs/import-event-state';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
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
  const location = useLocation();
  const { data: currentCase } = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const currentPage = useUiStore((state) => state.currentPage);
  const setCurrentPage = useUiStore((state) => state.setCurrentPage);
  const globalSearchQuery = useUiStore((state) => state.globalSearchQuery);
  const setGlobalSearchQuery = useUiStore((state) => state.setGlobalSearchQuery);
  const importSignals = useImportEventState();

  const runningCount = jobs?.filter((job) => job.status === 'running').length ?? 0;
  const warningCount = warnings?.length ?? 0;
  const partialCount = importSignals.partialResults.length;
  const freshestPartial = importSignals.partialResults[0];
  const cacheSummary = importSignals.cacheStatuses[0];
  const cancellation = importSignals.latestCancellation;
  const phase = importSignals.latestPhase;
  const report = importSignals.latestReport;
  const evidenceHashStatus = deriveEvidenceHashStatus(importSignals.partialResults, dataSources ?? []);
  const activeLink =
    pageKeys.find((link) => link.to === location.pathname)
    ?? pageKeys.find((link) => link.page === currentPage)
    ?? pageKeys[0];

  return (
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel px-4 py-2 text-xs">
      <div className="flex items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-6">
          <div className="flex items-center gap-5 min-w-0 overflow-x-auto scrollbar-none">
            {pageKeys.map((link) => (
              <NavLink
                key={link.to}
                to={link.to}
                onClick={() => setCurrentPage(link.page)}
                className={({ isActive }) =>
                  `whitespace-nowrap hover:text-forensics-text ${isActive ? 'text-forensics-text font-semibold' : 'text-forensics-muted'}`
                }
              >
                {t(`topBar.links.${link.page}.label`)}
              </NavLink>
            ))}
          </div>
          <div className="hidden xl:flex items-center gap-2 min-w-0 border-l border-forensics-border pl-4">
            <span className="text-[10px] uppercase tracking-wider text-forensics-muted-light">{t('topBar.currentPage')}</span>
            <span className="text-[11px] text-forensics-text font-medium">{t(`topBar.links.${activeLink.page}.context`)}</span>
          </div>
        </div>

        <div className="hidden lg:flex min-w-0 items-center gap-2 text-[12px] text-forensics-text">
          <span className="font-serif">{t('topBar.case.number', { number: currentCase?.number ?? '----' })}</span>
          <span className="max-w-[220px] truncate text-forensics-muted">{currentCase?.name ?? t('topBar.case.noCase')}</span>
          <span className="text-forensics-border-strong">|</span>
          <span className="text-forensics-muted">{t('topBar.case.examiner', { examiner: currentCase?.examiner ?? '-' })}</span>
          <span className="text-forensics-border-strong">|</span>
          <span className="font-mono text-forensics-muted-light">{t('topBar.case.updatedAt', { updatedAt: currentCase?.updatedAt ?? '-' })}</span>
        </div>

        <div className="flex shrink-0 items-center gap-3 text-forensics-muted">
          {phase ? (
            <SignalChip
              label="Import"
              value={`${getImportPhaseLabel(phase.phase)} ${phase.percent}%`}
              detail={`${getImportPhaseStateLabel(phase.state)} · ${phase.detail}`}
            />
          ) : null}
          {cancellation ? (
            <SignalChip
              label="Cancel"
              value={cancellation.safeToClose ? 'Safe To Close' : getCacheStateLabel(cancellation.state)}
              detail={cancellation.detail}
            />
          ) : null}
          {freshestPartial ? (
            <SignalChip
              label="Partial"
              value={`${getFreshnessLabel(freshestPartial.freshness)} ${partialCount}`}
              detail={`${getPartialKindLabel(freshestPartial.kind)} ${freshestPartial.readyCount}${freshestPartial.totalEstimate ? `/${freshestPartial.totalEstimate}` : ''}`}
            />
          ) : null}
          {evidenceHashStatus ? (
            <SignalChip
              label="Hash"
              value={getEvidenceHashStatusLabel(evidenceHashStatus)}
              detail={getEvidenceHashCaveatText(evidenceHashStatus)}
            />
          ) : null}
          {cacheSummary ? (
            <SignalChip
              label="Cache"
              value={getCacheStateLabel(cacheSummary.state)}
              detail={cacheSummary.message ?? cacheSummary.cacheKey}
            />
          ) : null}
          {report ? (
            <SignalChip
              label="Perf"
              value={`${report.summary.elapsedMs}ms`}
              detail={report.summary.summary}
            />
          ) : null}
          <div className="flex items-center gap-2 border border-forensics-border bg-forensics-surface px-2 py-1 rounded-sm">
            <Search size={12} className="text-forensics-muted-light" />
            <input
              value={globalSearchQuery}
              onChange={(event) => setGlobalSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && globalSearchQuery.trim()) {
                  navigate(`/search?q=${encodeURIComponent(globalSearchQuery.trim())}`);
                }
              }}
              className="w-40 xl:w-56 bg-transparent border-none outline-none text-forensics-text placeholder-forensics-500 text-xs font-mono"
              placeholder={t('topBar.search.placeholder')}
            />
          </div>
          <button
            onClick={toggleDrawer}
            className="flex items-center gap-1.5 border border-transparent px-2 py-1 hover:border-forensics-border hover:bg-forensics-surface text-forensics-text-tertiary hover:text-forensics-text"
          >
            <Activity size={12} />
            <span>{t('topBar.jobs.running', { count: runningCount })}</span>
            {warningCount > 0 ? (
              <span className="flex items-center gap-1 text-[#9a6700]">
                <AlertTriangle size={11} /> {warningCount}
              </span>
            ) : null}
          </button>
          <div className="h-4 border-l border-forensics-border" />
          <Settings size={14} className="cursor-pointer hover:text-forensics-text" onClick={() => navigate('/settings')} />
        </div>
      </div>
    </div>
  );
}

function SignalChip({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div
      className="hidden 2xl:flex max-w-[220px] items-center gap-2 border border-forensics-border bg-forensics-surface px-2 py-1"
      title={detail}
    >
      <span className="text-[10px] uppercase tracking-wider text-forensics-muted-light">{label}</span>
      <span className="truncate text-[11px] font-medium text-forensics-text">{value}</span>
    </div>
  );
}
