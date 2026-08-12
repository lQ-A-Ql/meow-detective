import { useTranslation } from 'react-i18next';
import type { LinuxArtifactSummary } from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { ExtractionTableSection } from './helpers';
import { useLinuxColumns } from './linuxColumns';
import { LinuxOverviewSection } from './LinuxOverviewSection';

export type LinuxArtifactTabKey =
  | 'overview'
  | 'journal'
  | 'login'
  | 'commands'
  | 'packages'
  | 'cron'
  | 'sudo'
  | 'systemConfig'
  | 'webServices'
  | 'mysqlServices';

const TABS: LinuxArtifactTabKey[] = [
  'overview',
  'journal',
  'login',
  'commands',
  'packages',
  'cron',
  'sudo',
  'systemConfig',
  'webServices',
  'mysqlServices',
];

interface LinuxFamilyProgress {
  loaded: number;
  total: number;
}

/** "已加载 X / 共 Y" plus a truncation warning when the backend capped results. */
function LoadProgressLine({
  loaded,
  total,
  truncated,
}: {
  loaded: number;
  total: number;
  truncated: boolean;
}) {
  const { t } = useTranslation();
  const showTruncated = truncated && total > 0;
  if (loaded >= total && !showTruncated) {
    return null;
  }
  return (
    <div className="flex flex-wrap items-center gap-2 px-1 text-[11px]">
      <span className="font-mono text-forensics-muted">
        {t('linuxArtifacts.loadProgress', { loaded, total })}
      </span>
      {showTruncated ? (
        <span className="text-forensics-warning-text">
          {t('linuxArtifacts.truncatedWarning')}
        </span>
      ) : null}
    </div>
  );
}

export function LinuxArtifactsPanel({
  summary,
  activeTab = 'overview',
  extractionRunning = false,
  hasMore = false,
  loadingMore = false,
  loadMoreFailed = false,
  onLoadMore,
  onRetryLoadMore,
  loadContextKey,
  loadStateKey,
}: {
  summary?: LinuxArtifactSummary;
  activeTab?: LinuxArtifactTabKey;
  extractionRunning?: boolean;
  hasMore?: boolean;
  loadingMore?: boolean;
  loadMoreFailed?: boolean;
  onLoadMore?: () => void;
  onRetryLoadMore?: () => unknown;
  loadContextKey?: string;
  loadStateKey?: string | number;
}) {
  const { t } = useTranslation();

  const fallbackInfo: LinuxArtifactSummary = {
    status: 'unavailable',
    journalCount: 0,
    textLogCount: 0,
    loginCount: 0,
    bashCommandCount: 0,
    aptEventCount: 0,
    cronJobCount: 0,
    sudoEventCount: 0,
    systemConfigCount: 0,
    webSiteCount: 0,
    webAccessLogCount: 0,
    webErrorLogCount: 0,
    webFindingCount: 0,
    mysqlConfigCount: 0,
    mysqlLogCount: 0,
    mysqlFindingCount: 0,
    totalCount: 0,
    truncated: false,
    coverageRatio: 0,
    journalEntries: [],
    loginRecords: [],
    bashCommands: [],
    aptEvents: [],
    cronJobs: [],
    sudoEvents: [],
    systemConfigs: [],
    webSites: [],
    webAccessLogs: [],
    webErrorLogs: [],
    webFindings: [],
    mysqlConfigs: [],
    mysqlLogs: [],
    mysqlFindings: [],
    warnings: [t('linuxArtifacts.fallbackWarning')],
    generatedAt: '',
  };
  const info = summary
    ? {
        ...summary,
        journalEntries: summary.journalEntries ?? [],
        loginRecords: summary.loginRecords ?? [],
        bashCommands: summary.bashCommands ?? [],
        aptEvents: summary.aptEvents ?? [],
        cronJobs: summary.cronJobs ?? [],
        sudoEvents: summary.sudoEvents ?? [],
        systemConfigs: summary.systemConfigs ?? [],
        webSites: summary.webSites ?? [],
        webAccessLogs: summary.webAccessLogs ?? [],
        webErrorLogs: summary.webErrorLogs ?? [],
        webFindings: summary.webFindings ?? [],
        mysqlConfigs: summary.mysqlConfigs ?? [],
        mysqlLogs: summary.mysqlLogs ?? [],
        mysqlFindings: summary.mysqlFindings ?? [],
      }
    : fallbackInfo;

  // Loaded rows vs backend counts per family; the journal family combines
  // journald rows and text-log fallback lines.
  const families: Record<string, LinuxFamilyProgress> = {
    journal: { loaded: info.journalEntries.length, total: info.journalCount + info.textLogCount },
    login: { loaded: info.loginRecords.length, total: info.loginCount },
    commands: { loaded: info.bashCommands.length, total: info.bashCommandCount },
    packages: { loaded: info.aptEvents.length, total: info.aptEventCount },
    cron: { loaded: info.cronJobs.length, total: info.cronJobCount },
    sudo: { loaded: info.sudoEvents.length, total: info.sudoEventCount },
    systemConfig: { loaded: info.systemConfigs.length, total: info.systemConfigCount },
    webSites: { loaded: info.webSites.length, total: info.webSiteCount },
    webAccess: { loaded: info.webAccessLogs.length, total: info.webAccessLogCount },
    webError: { loaded: info.webErrorLogs.length, total: info.webErrorLogCount },
    webFindings: { loaded: info.webFindings.length, total: info.webFindingCount },
    mysqlConfigs: { loaded: info.mysqlConfigs.length, total: info.mysqlConfigCount },
    mysqlLogs: { loaded: info.mysqlLogs.length, total: info.mysqlLogCount },
    mysqlFindings: { loaded: info.mysqlFindings.length, total: info.mysqlFindingCount },
  };
  const tableLoadContextKey = JSON.stringify([loadContextKey ?? null, info.generatedAt]);

  const columns = useLinuxColumns();

  const renderFamilyTable = <T,>(
    family: LinuxFamilyProgress,
    rows: T[],
    tableColumns: DenseColumn<T>[],
    emptyTitle: string,
    emptyDescription: string,
  ) => (
    <div className="space-y-1">
      <LoadProgressLine
        loaded={family.loaded}
        total={family.total}
        truncated={info.truncated}
      />
      <DenseDataTableFrame rowCount={rows.length}>
        <DenseDataTable
          rows={rows}
          columns={tableColumns}
          getRowKey={(row) => (row as { artifactId: string }).artifactId}
          emptyTitle={emptyTitle}
          emptyDescription={emptyDescription}
          loadContextKey={tableLoadContextKey}
          loadStateKey={loadStateKey}
          hasMore={hasMore && family.loaded < family.total}
          loadingMore={loadingMore}
          loadMoreFailed={loadMoreFailed}
          onReachEnd={onLoadMore}
          onRetryLoadMore={onRetryLoadMore}
        />
      </DenseDataTableFrame>
    </div>
  );

  const tabContent: Record<LinuxArtifactTabKey, React.ReactNode> = {
    overview: <LinuxOverviewSection info={info} />,
    journal: renderFamilyTable(
      families.journal,
      info.journalEntries,
      columns.journal,
      t('linuxArtifacts.empty.journal.title'),
      t('linuxArtifacts.empty.journal.description'),
    ),
    login: renderFamilyTable(
      families.login,
      info.loginRecords,
      columns.login,
      t('linuxArtifacts.empty.login.title'),
      t('linuxArtifacts.empty.login.description'),
    ),
    commands: renderFamilyTable(
      families.commands,
      info.bashCommands,
      columns.command,
      t('linuxArtifacts.empty.commands.title'),
      t('linuxArtifacts.empty.commands.description'),
    ),
    packages: renderFamilyTable(
      families.packages,
      info.aptEvents,
      columns.package,
      t('linuxArtifacts.empty.packages.title'),
      t('linuxArtifacts.empty.packages.description'),
    ),
    cron: renderFamilyTable(
      families.cron,
      info.cronJobs,
      columns.cron,
      t('linuxArtifacts.empty.cron.title'),
      t('linuxArtifacts.empty.cron.description'),
    ),
    sudo: renderFamilyTable(
      families.sudo,
      info.sudoEvents,
      columns.sudo,
      t('linuxArtifacts.empty.sudo.title'),
      t('linuxArtifacts.empty.sudo.description'),
    ),
    systemConfig: renderFamilyTable(
      families.systemConfig,
      info.systemConfigs,
      columns.systemConfig,
      t('linuxArtifacts.empty.systemConfig.title'),
      t('linuxArtifacts.empty.systemConfig.description'),
    ),
    webServices: (
      <div className="space-y-3">
        {renderFamilyTable(
          families.webFindings,
          info.webFindings,
          columns.webFinding,
          t('linuxArtifacts.empty.webFindings.title'),
          t('linuxArtifacts.empty.webFindings.description'),
        )}
        {renderFamilyTable(
          families.webSites,
          info.webSites,
          columns.webSite,
          t('linuxArtifacts.empty.webSites.title'),
          t('linuxArtifacts.empty.webSites.description'),
        )}
        {renderFamilyTable(
          families.webAccess,
          info.webAccessLogs,
          columns.webAccess,
          t('linuxArtifacts.empty.webAccess.title'),
          t('linuxArtifacts.empty.webAccess.description'),
        )}
        {renderFamilyTable(
          families.webError,
          info.webErrorLogs,
          columns.webError,
          t('linuxArtifacts.empty.webError.title'),
          t('linuxArtifacts.empty.webError.description'),
        )}
      </div>
    ),

    mysqlServices: (
      <div className="space-y-3">
        {renderFamilyTable(
          families.mysqlFindings,
          info.mysqlFindings,
          columns.mysqlFinding,
          t('linuxArtifacts.empty.mysqlFindings.title'),
          t('linuxArtifacts.empty.mysqlFindings.description'),
        )}
        {renderFamilyTable(
          families.mysqlConfigs,
          info.mysqlConfigs,
          columns.mysqlConfig,
          t('linuxArtifacts.empty.mysqlConfigs.title'),
          t('linuxArtifacts.empty.mysqlConfigs.description'),
        )}
        {renderFamilyTable(
          families.mysqlLogs,
          info.mysqlLogs,
          columns.mysqlLog,
          t('linuxArtifacts.empty.mysqlLogs.title'),
          t('linuxArtifacts.empty.mysqlLogs.description'),
        )}
      </div>
    ),
  };
  return (
    <ExtractionTableSection
      title={t('linuxArtifacts.title')}
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={extractionRunning ? [] : info.warnings}
      stats={[]}
    >
      <div className="min-h-0">{tabContent[activeTab]}</div>
    </ExtractionTableSection>
  );
}

export const LINUX_ARTIFACT_TAB_KEYS = TABS;
