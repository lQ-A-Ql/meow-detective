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

export function LinuxArtifactsPanel({
  summary,
  activeTab = 'overview',
  extractionRunning = false,
}: {
  summary?: LinuxArtifactSummary;
  activeTab?: LinuxArtifactTabKey;
  extractionRunning?: boolean;
}) {
  const { t } = useTranslation();

  const fallbackInfo = {
    status: 'unavailable' as const,
    journalCount: 0,
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

  // Column titles are translated, so all column arrays are memoized on `t`
  // instead of being rebuilt on every render.
  const columns = useMemo(() => ({
    journal: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
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

  const tabContent: Record<LinuxArtifactTabKey, React.ReactNode> = {
    overview: info.totalCount === 0 ? <EmptyLine text={t('linuxArtifacts.empty.overview.description')} /> : null,
    journal: (
      <DenseDataTableFrame rowCount={info.journalEntries.length}>
        <DenseDataTable
          rows={info.journalEntries}
          columns={columns.journal}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.journal.title')}
          emptyDescription={t('linuxArtifacts.empty.journal.description')}
        />
      </DenseDataTableFrame>
    ),
    login: (
      <DenseDataTableFrame rowCount={info.loginRecords.length}>
        <DenseDataTable
          rows={info.loginRecords}
          columns={columns.login}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.login.title')}
          emptyDescription={t('linuxArtifacts.empty.login.description')}
        />
      </DenseDataTableFrame>
    ),
    commands: (
      <DenseDataTableFrame rowCount={info.bashCommands.length}>
        <DenseDataTable
          rows={info.bashCommands}
          columns={columns.command}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.commands.title')}
          emptyDescription={t('linuxArtifacts.empty.commands.description')}
        />
      </DenseDataTableFrame>
    ),
    packages: (
      <DenseDataTableFrame rowCount={info.aptEvents.length}>
        <DenseDataTable
          rows={info.aptEvents}
          columns={columns.package}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.packages.title')}
          emptyDescription={t('linuxArtifacts.empty.packages.description')}
        />
      </DenseDataTableFrame>
    ),
    cron: (
      <DenseDataTableFrame rowCount={info.cronJobs.length}>
        <DenseDataTable
          rows={info.cronJobs}
          columns={columns.cron}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.cron.title')}
          emptyDescription={t('linuxArtifacts.empty.cron.description')}
        />
      </DenseDataTableFrame>
    ),
    sudo: (
      <DenseDataTableFrame rowCount={info.sudoEvents.length}>
        <DenseDataTable
          rows={info.sudoEvents}
          columns={columns.sudo}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.sudo.title')}
          emptyDescription={t('linuxArtifacts.empty.sudo.description')}
        />
      </DenseDataTableFrame>
    ),
    systemConfig: (
      <DenseDataTableFrame rowCount={info.systemConfigs.length}>
        <DenseDataTable
          rows={info.systemConfigs}
          columns={columns.systemConfig}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.systemConfig.title')}
          emptyDescription={t('linuxArtifacts.empty.systemConfig.description')}
        />
      </DenseDataTableFrame>
    ),
    webServices: (
      <div className="space-y-3">
        <DenseDataTableFrame rowCount={info.webFindings.length}>
          <DenseDataTable
            rows={info.webFindings}
            columns={columns.webFinding}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.webFindings.title')}
            emptyDescription={t('linuxArtifacts.empty.webFindings.description')}
          />
        </DenseDataTableFrame>
        <DenseDataTableFrame rowCount={info.webSites.length}>
          <DenseDataTable
            rows={info.webSites}
            columns={columns.webSite}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.webSites.title')}
            emptyDescription={t('linuxArtifacts.empty.webSites.description')}
          />
        </DenseDataTableFrame>
        <DenseDataTableFrame rowCount={info.webAccessLogs.length}>
          <DenseDataTable
            rows={info.webAccessLogs}
            columns={columns.webAccess}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.webAccess.title')}
            emptyDescription={t('linuxArtifacts.empty.webAccess.description')}
          />
        </DenseDataTableFrame>
        <DenseDataTableFrame rowCount={info.webErrorLogs.length}>
          <DenseDataTable
            rows={info.webErrorLogs}
            columns={columns.webError}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.webError.title')}
            emptyDescription={t('linuxArtifacts.empty.webError.description')}
          />
        </DenseDataTableFrame>
      </div>
    ),

    mysqlServices: (
      <div className="space-y-3">
        <DenseDataTableFrame rowCount={info.mysqlFindings.length}>
          <DenseDataTable
            rows={info.mysqlFindings}
            columns={columns.mysqlFinding}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.mysqlFindings.title')}
            emptyDescription={t('linuxArtifacts.empty.mysqlFindings.description')}
          />
        </DenseDataTableFrame>
        <DenseDataTableFrame rowCount={info.mysqlConfigs.length}>
          <DenseDataTable
            rows={info.mysqlConfigs}
            columns={columns.mysqlConfig}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.mysqlConfigs.title')}
            emptyDescription={t('linuxArtifacts.empty.mysqlConfigs.description')}
          />
        </DenseDataTableFrame>
        <DenseDataTableFrame rowCount={info.mysqlLogs.length}>
          <DenseDataTable
            rows={info.mysqlLogs}
            columns={columns.mysqlLog}
            getRowKey={(row) => row.artifactId}
            emptyTitle={t('linuxArtifacts.empty.mysqlLogs.title')}
            emptyDescription={t('linuxArtifacts.empty.mysqlLogs.description')}
          />
        </DenseDataTableFrame>
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
