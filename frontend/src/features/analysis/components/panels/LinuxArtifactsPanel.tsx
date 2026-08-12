import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type {
  LinuxAptEvent,
  LinuxArtifactSummary,
  LinuxBashCommand,
  LinuxCronJob,
  LinuxJournalEntry,
  LinuxLoginRecord,
  LinuxMysqlConfig,
  LinuxMysqlFinding,
  LinuxMysqlLogEntry,
  LinuxSudoEvent,
  LinuxSystemConfig,
  LinuxWebAccessLog,
  LinuxWebErrorLog,
  LinuxWebFinding,
  LinuxWebSite,
} from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { EmptyLine, ExtractionTableSection } from './helpers';

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

  // Column titles are translated, so all column arrays are memoized on `t`
  // instead of being rebuilt on every render.
  const columns = useMemo(() => ({
    journal: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'logKind', title: t('linuxArtifacts.columns.logKind'), className: 'w-[100px]', render: (row) => row.logKind ?? 'journald' },
    { key: 'priority', title: t('linuxArtifacts.columns.priority'), className: 'w-[70px]', render: (row) => row.priority?.toString() ?? '-' },
    { key: 'systemdUnit', title: t('linuxArtifacts.columns.systemdUnit'), className: 'w-[140px]', render: (row) => row.systemdUnit ?? '-' },
    { key: 'syslogIdentifier', title: t('linuxArtifacts.columns.syslogIdentifier'), className: 'w-[120px]', render: (row) => row.syslogIdentifier ?? '-' },
    { key: 'pid', title: t('linuxArtifacts.columns.pid'), className: 'w-[70px]', render: (row) => row.pid?.toString() ?? '-' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[240px]', render: (row) => row.message ?? '-' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxJournalEntry>[],
    login: [
    { key: 'loginTime', title: t('linuxArtifacts.columns.loginTime'), className: 'w-[180px]', render: (row) => row.loginTime ?? '-' },
    { key: 'logoutTime', title: t('linuxArtifacts.columns.logoutTime'), className: 'w-[180px]', render: (row) => row.logoutTime ?? '-' },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[120px]', render: (row) => row.user },
    { key: 'terminal', title: t('linuxArtifacts.columns.terminal'), className: 'w-[100px]', render: (row) => row.terminal },
    { key: 'host', title: t('linuxArtifacts.columns.host'), className: 'w-[140px]', render: (row) => row.host },
    { key: 'recordType', title: t('linuxArtifacts.columns.recordType'), className: 'w-[80px]', render: (row) => row.recordType.toString() },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxLoginRecord>[],
    command: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[300px]', render: (row) => <span className="font-mono">{row.command}</span> },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString() },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxBashCommand>[],
    package: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'action', title: t('linuxArtifacts.columns.action'), className: 'w-[100px]', render: (row) => row.action },
    { key: 'package', title: t('linuxArtifacts.columns.package'), className: 'min-w-[200px]', render: (row) => row.package },
    { key: 'version', title: t('linuxArtifacts.columns.version'), className: 'w-[160px]', render: (row) => row.version },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxAptEvent>[],
    cron: [
    { key: 'schedule', title: t('linuxArtifacts.columns.schedule'), className: 'w-[140px]', render: (row) => <span className="font-mono">{row.schedule}</span> },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[100px]', render: (row) => row.user ?? '-' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[260px]', render: (row) => <span className="font-mono">{row.command}</span> },
    { key: 'sourceFile', title: t('linuxArtifacts.columns.sourceFile'), className: 'min-w-[180px]', render: (row) => row.sourceFile },
    ] as DenseColumn<LinuxCronJob>[],
    sudo: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[100px]', render: (row) => row.user },
    { key: 'targetUser', title: t('linuxArtifacts.columns.targetUser'), className: 'w-[100px]', render: (row) => row.targetUser ?? '-' },
    {
      key: 'success',
      title: t('linuxArtifacts.columns.success'),
      className: 'w-[80px]',
      render: (row) => (
        <span
          className={
            row.success
              ? 'rounded-none bg-forensics-success-bg px-1.5 py-0.5 text-[10px] text-forensics-success-text'
              : 'rounded-none bg-forensics-error-bg px-1.5 py-0.5 text-[10px] text-forensics-error-text'
          }
        >
          {row.success ? t('linuxArtifacts.columns.success') : '✕'}
        </span>
      ),
    },
    { key: 'terminal', title: t('linuxArtifacts.columns.terminal'), className: 'w-[90px]', render: (row) => row.terminal ?? '-' },
    { key: 'workingDirectory', title: t('linuxArtifacts.columns.workingDirectory'), className: 'w-[160px]', render: (row) => row.workingDirectory ?? '-' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[240px]', render: (row) => <span className="font-mono">{row.command}</span> },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxSudoEvent>[],
    systemConfig: [
    { key: 'configKind', title: t('linuxArtifacts.columns.configKind'), className: 'w-[130px]', render: (row) => row.configKind || '-' },
    { key: 'key', title: t('linuxArtifacts.columns.key'), className: 'w-[140px]', render: (row) => row.key ?? row.username ?? '-' },
    { key: 'value', title: t('linuxArtifacts.columns.value'), className: 'min-w-[220px]', render: (row) => row.value ?? row.line ?? '-' },
    { key: 'uid', title: t('linuxArtifacts.columns.uid'), className: 'w-[70px]', render: (row) => row.uid?.toString() ?? '-' },
    { key: 'gid', title: t('linuxArtifacts.columns.gid'), className: 'w-[70px]', render: (row) => row.gid?.toString() ?? '-' },
    { key: 'home', title: t('linuxArtifacts.columns.home'), className: 'w-[160px]', render: (row) => row.home ?? '-' },
    { key: 'shell', title: t('linuxArtifacts.columns.shell'), className: 'w-[140px]', render: (row) => row.shell ?? '-' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxSystemConfig>[],
    webSite: [
    { key: 'serverKind', title: t('linuxArtifacts.columns.serverKind'), className: 'w-[90px]', render: (row) => row.serverKind },
    { key: 'siteName', title: t('linuxArtifacts.columns.siteName'), className: 'min-w-[160px]', render: (row) => row.siteName },
    { key: 'hostnames', title: t('linuxArtifacts.columns.hostnames'), className: 'min-w-[180px]', render: (row) => row.hostnames.join(', ') || '-' },
    { key: 'listen', title: t('linuxArtifacts.columns.listen'), className: 'w-[140px]', render: (row) => row.listen.join(', ') || '-' },
    { key: 'documentRoots', title: t('linuxArtifacts.columns.documentRoots'), className: 'min-w-[220px]', render: (row) => row.documentRoots.join(', ') || '-' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebSite>[],
    webAccess: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'clientIp', title: t('linuxArtifacts.columns.clientIp'), className: 'w-[130px]', render: (row) => row.clientIp },
    { key: 'method', title: t('linuxArtifacts.columns.method'), className: 'w-[70px]', render: (row) => row.method },
    { key: 'status', title: t('linuxArtifacts.columns.statusCode'), className: 'w-[80px]', render: (row) => row.status.toString() },
    { key: 'uri', title: t('linuxArtifacts.columns.uri'), className: 'min-w-[260px]', render: (row) => <span className="font-mono">{row.uri}</span> },
    { key: 'userAgent', title: t('linuxArtifacts.columns.userAgent'), className: 'min-w-[200px]', render: (row) => row.userAgent ?? '-' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebAccessLog>[],
    webError: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[110px]', render: (row) => row.severity ?? '-' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[320px]', render: (row) => row.message },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebErrorLog>[],
    webFinding: [
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[90px]', render: (row) => row.severity },
    { key: 'findingKind', title: t('linuxArtifacts.columns.findingKind'), className: 'w-[150px]', render: (row) => row.findingKind },
    { key: 'confidence', title: t('linuxArtifacts.columns.confidence'), className: 'w-[90px]', render: (row) => `${Math.round(row.confidence * 100)}%` },
    { key: 'clientIp', title: t('linuxArtifacts.columns.clientIp'), className: 'w-[130px]', render: (row) => row.clientIp ?? '-' },
    { key: 'uri', title: t('linuxArtifacts.columns.uri'), className: 'min-w-[220px]', render: (row) => row.uri ?? '-' },
    { key: 'evidence', title: t('linuxArtifacts.columns.evidence'), className: 'min-w-[260px]', render: (row) => row.evidence },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebFinding>[],
    mysqlFinding: [
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[90px]', render: (row) => row.severity },
    { key: 'findingKind', title: t('linuxArtifacts.columns.findingKind'), className: 'w-[170px]', render: (row) => row.findingKind },
    { key: 'confidence', title: t('linuxArtifacts.columns.confidence'), className: 'w-[90px]', render: (row) => `${Math.round(row.confidence * 100)}%` },
    { key: 'evidence', title: t('linuxArtifacts.columns.evidence'), className: 'min-w-[300px]', render: (row) => row.evidence },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString() },
    ] as DenseColumn<LinuxMysqlFinding>[],
    mysqlConfig: [
    { key: 'section', title: t('linuxArtifacts.columns.section'), className: 'w-[120px]', render: (row) => row.section ?? '-' },
    { key: 'key', title: t('linuxArtifacts.columns.key'), className: 'w-[180px]', render: (row) => <span className="font-mono">{row.key}</span> },
    { key: 'value', title: t('linuxArtifacts.columns.value'), className: 'min-w-[240px]', render: (row) => <span className="font-mono">{row.value}</span> },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString() },
    ] as DenseColumn<LinuxMysqlConfig>[],
    mysqlLog: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[110px]', render: (row) => row.severity ?? '-' },
    { key: 'threadId', title: t('linuxArtifacts.columns.threadId'), className: 'w-[90px]', render: (row) => row.threadId ?? '-' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[320px]', render: (row) => row.message },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString() },
    ] as DenseColumn<LinuxMysqlLogEntry>[],
  }), [t]);

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
    overview: info.totalCount === 0 ? <EmptyLine text={t('linuxArtifacts.empty.overview.description')} /> : null,
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
