import { useTranslation } from 'react-i18next';
import type {
  LinuxAptEvent,
  LinuxArtifactSummary,
  LinuxBashCommand,
  LinuxCronJob,
  LinuxJournalEntry,
  LinuxLoginRecord,
  LinuxSudoEvent,
} from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  AnalysisExtractionProgress,
  type AnalysisExtractionProgressInfo,
  DenseTableFrame,
  EmptyLine,
  ExtractionTableSection,
  StatCard,
} from './helpers';

export type LinuxArtifactTabKey =
  | 'overview'
  | 'journal'
  | 'login'
  | 'commands'
  | 'packages'
  | 'cron'
  | 'sudo';

const TABS: LinuxArtifactTabKey[] = [
  'overview',
  'journal',
  'login',
  'commands',
  'packages',
  'cron',
  'sudo',
];

export function LinuxArtifactsPanel({
  summary,
  progress,
  activeTab = 'overview',
}: {
  summary?: LinuxArtifactSummary;
  progress?: AnalysisExtractionProgressInfo;
  activeTab?: LinuxArtifactTabKey;
}) {
  const { t } = useTranslation();

  const info = summary ?? {
    status: 'unavailable' as const,
    journalCount: 0,
    loginCount: 0,
    bashCommandCount: 0,
    aptEventCount: 0,
    cronJobCount: 0,
    sudoEventCount: 0,
    totalCount: 0,
    truncated: false,
    coverageRatio: 0,
    journalEntries: [],
    loginRecords: [],
    bashCommands: [],
    aptEvents: [],
    cronJobs: [],
    sudoEvents: [],
    warnings: [t('linuxArtifacts.fallbackWarning')],
    generatedAt: '',
  };

  const journalColumns: DenseColumn<LinuxJournalEntry>[] = [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'priority', title: t('linuxArtifacts.columns.priority'), className: 'w-[70px]', render: (row) => row.priority?.toString() ?? '-' },
    { key: 'systemdUnit', title: t('linuxArtifacts.columns.systemdUnit'), className: 'w-[140px]', render: (row) => row.systemdUnit ?? '-' },
    { key: 'syslogIdentifier', title: t('linuxArtifacts.columns.syslogIdentifier'), className: 'w-[120px]', render: (row) => row.syslogIdentifier ?? '-' },
    { key: 'pid', title: t('linuxArtifacts.columns.pid'), className: 'w-[70px]', render: (row) => row.pid?.toString() ?? '-' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[240px]', render: (row) => row.message ?? '-' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
  ];

  const loginColumns: DenseColumn<LinuxLoginRecord>[] = [
    { key: 'loginTime', title: t('linuxArtifacts.columns.loginTime'), className: 'w-[180px]', render: (row) => row.loginTime ?? '-' },
    { key: 'logoutTime', title: t('linuxArtifacts.columns.logoutTime'), className: 'w-[180px]', render: (row) => row.logoutTime ?? '-' },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[120px]', render: (row) => row.user },
    { key: 'terminal', title: t('linuxArtifacts.columns.terminal'), className: 'w-[100px]', render: (row) => row.terminal },
    { key: 'host', title: t('linuxArtifacts.columns.host'), className: 'w-[140px]', render: (row) => row.host },
    { key: 'recordType', title: t('linuxArtifacts.columns.recordType'), className: 'w-[80px]', render: (row) => row.recordType.toString() },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
  ];

  const commandColumns: DenseColumn<LinuxBashCommand>[] = [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[300px]', render: (row) => <span className="font-mono">{row.command}</span> },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString() },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
  ];

  const packageColumns: DenseColumn<LinuxAptEvent>[] = [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-' },
    { key: 'action', title: t('linuxArtifacts.columns.action'), className: 'w-[100px]', render: (row) => row.action },
    { key: 'package', title: t('linuxArtifacts.columns.package'), className: 'min-w-[200px]', render: (row) => row.package },
    { key: 'version', title: t('linuxArtifacts.columns.version'), className: 'w-[160px]', render: (row) => row.version },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath },
  ];

  const cronColumns: DenseColumn<LinuxCronJob>[] = [
    { key: 'schedule', title: t('linuxArtifacts.columns.schedule'), className: 'w-[140px]', render: (row) => <span className="font-mono">{row.schedule}</span> },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[100px]', render: (row) => row.user ?? '-' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[260px]', render: (row) => <span className="font-mono">{row.command}</span> },
    { key: 'sourceFile', title: t('linuxArtifacts.columns.sourceFile'), className: 'min-w-[180px]', render: (row) => row.sourceFile },
  ];

  const sudoColumns: DenseColumn<LinuxSudoEvent>[] = [
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
              ? 'rounded bg-[#e6f4ea] px-1.5 py-0.5 text-[10px] text-[#0f7b3c]'
              : 'rounded bg-[#fdecea] px-1.5 py-0.5 text-[10px] text-[#c02626]'
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
  ];

  const overviewContent = (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
      <StatCard label={t('linuxArtifacts.stats.journal')} value={info.journalCount.toString()} />
      <StatCard label={t('linuxArtifacts.stats.login')} value={info.loginCount.toString()} />
      <StatCard label={t('linuxArtifacts.stats.commands')} value={info.bashCommandCount.toString()} />
      <StatCard label={t('linuxArtifacts.stats.packages')} value={info.aptEventCount.toString()} />
      <StatCard label={t('linuxArtifacts.stats.cron')} value={info.cronJobCount.toString()} />
      <StatCard label={t('linuxArtifacts.stats.sudo')} value={info.sudoEventCount.toString()} />
    </div>
  );

  const tabContent: Record<LinuxArtifactTabKey, React.ReactNode> = {
    overview:
      info.totalCount === 0 ? (
        <EmptyLine text={t('linuxArtifacts.empty.overview.description')} />
      ) : (
        overviewContent
      ),
    journal: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.journalEntries}
          columns={journalColumns}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.journal.title')}
          emptyDescription={t('linuxArtifacts.empty.journal.description')}
        />
      </DenseTableFrame>
    ),
    login: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.loginRecords}
          columns={loginColumns}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.login.title')}
          emptyDescription={t('linuxArtifacts.empty.login.description')}
        />
      </DenseTableFrame>
    ),
    commands: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.bashCommands}
          columns={commandColumns}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.commands.title')}
          emptyDescription={t('linuxArtifacts.empty.commands.description')}
        />
      </DenseTableFrame>
    ),
    packages: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.aptEvents}
          columns={packageColumns}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.packages.title')}
          emptyDescription={t('linuxArtifacts.empty.packages.description')}
        />
      </DenseTableFrame>
    ),
    cron: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.cronJobs}
          columns={cronColumns}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.cron.title')}
          emptyDescription={t('linuxArtifacts.empty.cron.description')}
        />
      </DenseTableFrame>
    ),
    sudo: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.sudoEvents}
          columns={sudoColumns}
          getRowKey={(row) => row.artifactId}
          emptyTitle={t('linuxArtifacts.empty.sudo.title')}
          emptyDescription={t('linuxArtifacts.empty.sudo.description')}
        />
      </DenseTableFrame>
    ),
  };

  return (
    <ExtractionTableSection
      title={t('linuxArtifacts.title')}
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        [t('linuxArtifacts.stats.total'), info.totalCount.toString()],
        [t('linuxArtifacts.stats.journal'), info.journalCount.toString()],
        [t('linuxArtifacts.stats.login'), info.loginCount.toString()],
        [t('linuxArtifacts.stats.commands'), info.bashCommandCount.toString()],
        [t('linuxArtifacts.stats.coverage'), `${Math.round(info.coverageRatio * 100)}%`],
        [t('linuxArtifacts.stats.truncated'), info.truncated ? t('linuxArtifacts.values.yes') : t('linuxArtifacts.values.no')],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />

      <div className="min-h-0">{tabContent[activeTab]}</div>
    </ExtractionTableSection>
  );
}

export const LINUX_ARTIFACT_TAB_KEYS = TABS;
