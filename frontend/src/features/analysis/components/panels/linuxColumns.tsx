import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type {
  LinuxAptEvent,
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
import type { DenseColumn } from '@/components/tables/DenseDataTable';

// Column titles are translated, so all column arrays are memoized on `t`
// instead of being rebuilt on every render. Every column also declares a
// `text` accessor so the DenseDataTable filter bar can keyword-match rows;
// low-cardinality columns are marked `filterable` to get a dropdown.
export function useLinuxColumns() {
  const { t } = useTranslation();
  return useMemo(() => ({
    journal: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'logKind', title: t('linuxArtifacts.columns.logKind'), className: 'w-[100px]', filterable: true, render: (row) => row.logKind ?? 'journald', text: (row) => row.logKind ?? 'journald' },
    { key: 'priority', title: t('linuxArtifacts.columns.priority'), className: 'w-[70px]', filterable: true, render: (row) => row.priority?.toString() ?? '-', text: (row) => row.priority?.toString() ?? '-' },
    { key: 'systemdUnit', title: t('linuxArtifacts.columns.systemdUnit'), className: 'w-[140px]', render: (row) => row.systemdUnit ?? '-', text: (row) => row.systemdUnit ?? '' },
    { key: 'syslogIdentifier', title: t('linuxArtifacts.columns.syslogIdentifier'), className: 'w-[120px]', render: (row) => row.syslogIdentifier ?? '-', text: (row) => row.syslogIdentifier ?? '' },
    { key: 'pid', title: t('linuxArtifacts.columns.pid'), className: 'w-[70px]', render: (row) => row.pid?.toString() ?? '-', text: (row) => row.pid?.toString() ?? '' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[240px]', render: (row) => row.message ?? '-', text: (row) => row.message ?? '' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxJournalEntry>[],
    login: [
    { key: 'loginTime', title: t('linuxArtifacts.columns.loginTime'), className: 'w-[180px]', render: (row) => row.loginTime ?? '-', text: (row) => row.loginTime ?? '' },
    { key: 'logoutTime', title: t('linuxArtifacts.columns.logoutTime'), className: 'w-[180px]', render: (row) => row.logoutTime ?? '-', text: (row) => row.logoutTime ?? '' },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[120px]', render: (row) => row.user, text: (row) => row.user },
    { key: 'terminal', title: t('linuxArtifacts.columns.terminal'), className: 'w-[100px]', filterable: true, render: (row) => row.terminal, text: (row) => row.terminal },
    { key: 'host', title: t('linuxArtifacts.columns.host'), className: 'w-[140px]', render: (row) => row.host, text: (row) => row.host },
    { key: 'recordType', title: t('linuxArtifacts.columns.recordType'), className: 'w-[80px]', render: (row) => row.recordType.toString(), text: (row) => row.recordType.toString() },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxLoginRecord>[],
    command: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[300px]', render: (row) => <span className="font-mono">{row.command}</span>, text: (row) => row.command },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString(), text: (row) => row.lineNumber.toString() },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxBashCommand>[],
    package: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'action', title: t('linuxArtifacts.columns.action'), className: 'w-[100px]', filterable: true, render: (row) => row.action, text: (row) => row.action },
    { key: 'package', title: t('linuxArtifacts.columns.package'), className: 'min-w-[200px]', render: (row) => row.package, text: (row) => row.package },
    { key: 'version', title: t('linuxArtifacts.columns.version'), className: 'w-[160px]', render: (row) => row.version, text: (row) => row.version },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxAptEvent>[],
    cron: [
    { key: 'schedule', title: t('linuxArtifacts.columns.schedule'), className: 'w-[140px]', render: (row) => <span className="font-mono">{row.schedule}</span>, text: (row) => row.schedule },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[100px]', filterable: true, render: (row) => row.user ?? '-', text: (row) => row.user ?? '-' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[260px]', render: (row) => <span className="font-mono">{row.command}</span>, text: (row) => row.command },
    { key: 'sourceFile', title: t('linuxArtifacts.columns.sourceFile'), className: 'min-w-[180px]', render: (row) => row.sourceFile, text: (row) => row.sourceFile },
    ] as DenseColumn<LinuxCronJob>[],
    sudo: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'user', title: t('linuxArtifacts.columns.user'), className: 'w-[100px]', render: (row) => row.user, text: (row) => row.user },
    { key: 'targetUser', title: t('linuxArtifacts.columns.targetUser'), className: 'w-[100px]', render: (row) => row.targetUser ?? '-', text: (row) => row.targetUser ?? '' },
    {
      key: 'success',
      title: t('linuxArtifacts.columns.success'),
      className: 'w-[80px]',
      filterable: true,
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
      text: (row) => row.success ? t('linuxArtifacts.values.yes') : t('linuxArtifacts.values.no'),
    },
    { key: 'terminal', title: t('linuxArtifacts.columns.terminal'), className: 'w-[90px]', render: (row) => row.terminal ?? '-', text: (row) => row.terminal ?? '' },
    { key: 'workingDirectory', title: t('linuxArtifacts.columns.workingDirectory'), className: 'w-[160px]', render: (row) => row.workingDirectory ?? '-', text: (row) => row.workingDirectory ?? '' },
    { key: 'command', title: t('linuxArtifacts.columns.command'), className: 'min-w-[240px]', render: (row) => <span className="font-mono">{row.command}</span>, text: (row) => row.command },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxSudoEvent>[],
    systemConfig: [
    { key: 'configKind', title: t('linuxArtifacts.columns.configKind'), className: 'w-[130px]', filterable: true, render: (row) => row.configKind || '-', text: (row) => row.configKind || '-' },
    { key: 'key', title: t('linuxArtifacts.columns.key'), className: 'w-[140px]', render: (row) => row.key ?? row.username ?? '-', text: (row) => row.key ?? row.username ?? '' },
    { key: 'value', title: t('linuxArtifacts.columns.value'), className: 'min-w-[220px]', render: (row) => row.value ?? row.line ?? '-', text: (row) => row.value ?? row.line ?? '' },
    { key: 'uid', title: t('linuxArtifacts.columns.uid'), className: 'w-[70px]', render: (row) => row.uid?.toString() ?? '-', text: (row) => row.uid?.toString() ?? '' },
    { key: 'gid', title: t('linuxArtifacts.columns.gid'), className: 'w-[70px]', render: (row) => row.gid?.toString() ?? '-', text: (row) => row.gid?.toString() ?? '' },
    { key: 'home', title: t('linuxArtifacts.columns.home'), className: 'w-[160px]', render: (row) => row.home ?? '-', text: (row) => row.home ?? '' },
    { key: 'shell', title: t('linuxArtifacts.columns.shell'), className: 'w-[140px]', render: (row) => row.shell ?? '-', text: (row) => row.shell ?? '' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxSystemConfig>[],
    webSite: [
    { key: 'serverKind', title: t('linuxArtifacts.columns.serverKind'), className: 'w-[90px]', render: (row) => row.serverKind, text: (row) => row.serverKind },
    { key: 'siteName', title: t('linuxArtifacts.columns.siteName'), className: 'min-w-[160px]', render: (row) => row.siteName, text: (row) => row.siteName },
    { key: 'hostnames', title: t('linuxArtifacts.columns.hostnames'), className: 'min-w-[180px]', render: (row) => row.hostnames.join(', ') || '-', text: (row) => row.hostnames.join(', ') },
    { key: 'listen', title: t('linuxArtifacts.columns.listen'), className: 'w-[140px]', render: (row) => row.listen.join(', ') || '-', text: (row) => row.listen.join(', ') },
    { key: 'documentRoots', title: t('linuxArtifacts.columns.documentRoots'), className: 'min-w-[220px]', render: (row) => row.documentRoots.join(', ') || '-', text: (row) => row.documentRoots.join(', ') },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebSite>[],
    webAccess: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'clientIp', title: t('linuxArtifacts.columns.clientIp'), className: 'w-[130px]', render: (row) => row.clientIp, text: (row) => row.clientIp },
    { key: 'method', title: t('linuxArtifacts.columns.method'), className: 'w-[70px]', filterable: true, render: (row) => row.method, text: (row) => row.method },
    { key: 'status', title: t('linuxArtifacts.columns.statusCode'), className: 'w-[80px]', filterable: true, render: (row) => row.status.toString(), text: (row) => row.status.toString() },
    { key: 'uri', title: t('linuxArtifacts.columns.uri'), className: 'min-w-[260px]', render: (row) => <span className="font-mono">{row.uri}</span>, text: (row) => row.uri },
    { key: 'userAgent', title: t('linuxArtifacts.columns.userAgent'), className: 'min-w-[200px]', render: (row) => row.userAgent ?? '-', text: (row) => row.userAgent ?? '' },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebAccessLog>[],
    webError: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[110px]', filterable: true, render: (row) => row.severity ?? '-', text: (row) => row.severity ?? '-' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[320px]', render: (row) => row.message, text: (row) => row.message },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebErrorLog>[],
    webFinding: [
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[90px]', filterable: true, render: (row) => row.severity, text: (row) => row.severity },
    { key: 'findingKind', title: t('linuxArtifacts.columns.findingKind'), className: 'w-[150px]', render: (row) => row.findingKind, text: (row) => row.findingKind },
    { key: 'confidence', title: t('linuxArtifacts.columns.confidence'), className: 'w-[90px]', render: (row) => `${Math.round(row.confidence * 100)}%`, text: (row) => `${Math.round(row.confidence * 100)}%` },
    { key: 'clientIp', title: t('linuxArtifacts.columns.clientIp'), className: 'w-[130px]', render: (row) => row.clientIp ?? '-', text: (row) => row.clientIp ?? '' },
    { key: 'uri', title: t('linuxArtifacts.columns.uri'), className: 'min-w-[220px]', render: (row) => row.uri ?? '-', text: (row) => row.uri ?? '' },
    { key: 'evidence', title: t('linuxArtifacts.columns.evidence'), className: 'min-w-[260px]', render: (row) => row.evidence, text: (row) => row.evidence },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    ] as DenseColumn<LinuxWebFinding>[],
    mysqlFinding: [
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[90px]', filterable: true, render: (row) => row.severity, text: (row) => row.severity },
    { key: 'findingKind', title: t('linuxArtifacts.columns.findingKind'), className: 'w-[170px]', render: (row) => row.findingKind, text: (row) => row.findingKind },
    { key: 'confidence', title: t('linuxArtifacts.columns.confidence'), className: 'w-[90px]', render: (row) => `${Math.round(row.confidence * 100)}%`, text: (row) => `${Math.round(row.confidence * 100)}%` },
    { key: 'evidence', title: t('linuxArtifacts.columns.evidence'), className: 'min-w-[300px]', render: (row) => row.evidence, text: (row) => row.evidence },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString(), text: (row) => row.lineNumber.toString() },
    ] as DenseColumn<LinuxMysqlFinding>[],
    mysqlConfig: [
    { key: 'section', title: t('linuxArtifacts.columns.section'), className: 'w-[120px]', render: (row) => row.section ?? '-', text: (row) => row.section ?? '' },
    { key: 'key', title: t('linuxArtifacts.columns.key'), className: 'w-[180px]', render: (row) => <span className="font-mono">{row.key}</span>, text: (row) => row.key },
    { key: 'value', title: t('linuxArtifacts.columns.value'), className: 'min-w-[240px]', render: (row) => <span className="font-mono">{row.value}</span>, text: (row) => row.value },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString(), text: (row) => row.lineNumber.toString() },
    ] as DenseColumn<LinuxMysqlConfig>[],
    mysqlLog: [
    { key: 'timestamp', title: t('linuxArtifacts.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp ?? '-', text: (row) => row.timestamp ?? '' },
    { key: 'severity', title: t('linuxArtifacts.columns.severity'), className: 'w-[110px]', filterable: true, render: (row) => row.severity ?? '-', text: (row) => row.severity ?? '-' },
    { key: 'threadId', title: t('linuxArtifacts.columns.threadId'), className: 'w-[90px]', render: (row) => row.threadId ?? '-', text: (row) => row.threadId ?? '' },
    { key: 'message', title: t('linuxArtifacts.columns.message'), className: 'min-w-[320px]', render: (row) => row.message, text: (row) => row.message },
    { key: 'sourcePath', title: t('linuxArtifacts.columns.sourcePath'), className: 'min-w-[180px]', render: (row) => row.sourcePath, text: (row) => row.sourcePath },
    { key: 'lineNumber', title: t('linuxArtifacts.columns.lineNumber'), className: 'w-[80px]', render: (row) => row.lineNumber.toString(), text: (row) => row.lineNumber.toString() },
    ] as DenseColumn<LinuxMysqlLogEntry>[],
  }), [t]);
}

export type LinuxColumns = ReturnType<typeof useLinuxColumns>;
