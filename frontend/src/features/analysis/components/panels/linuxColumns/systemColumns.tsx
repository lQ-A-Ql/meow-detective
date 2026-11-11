import type { TFunction } from 'i18next';
import type { LinuxBashCommand, LinuxJournalEntry, LinuxLoginRecord } from '@/types/models';
import type { DenseColumn } from '@/components/tables/DenseDataTable';

export function buildJournalColumns(t: TFunction): DenseColumn<LinuxJournalEntry>[] {
  return [
    {
      key: 'timestamp',
      title: t('linuxArtifacts.columns.timestamp'),
      className: 'w-[180px]',
      render: (row) => row.timestamp ?? '-',
      text: (row) => row.timestamp ?? '',
    },
    {
      key: 'logKind',
      title: t('linuxArtifacts.columns.logKind'),
      className: 'w-[100px]',
      filterable: true,
      render: (row) => row.logKind ?? '-',
      text: (row) => row.logKind ?? '',
    },
    {
      key: 'priority',
      title: t('linuxArtifacts.columns.priority'),
      className: 'w-[70px]',
      filterable: true,
      render: (row) => row.priority?.toString() ?? '-',
      text: (row) => row.priority?.toString() ?? '-',
    },
    {
      key: 'systemdUnit',
      title: t('linuxArtifacts.columns.systemdUnit'),
      className: 'w-[140px]',
      render: (row) => row.systemdUnit ?? '-',
      text: (row) => row.systemdUnit ?? '',
    },
    {
      key: 'syslogIdentifier',
      title: t('linuxArtifacts.columns.syslogIdentifier'),
      className: 'w-[120px]',
      render: (row) => row.syslogIdentifier ?? '-',
      text: (row) => row.syslogIdentifier ?? '',
    },
    {
      key: 'pid',
      title: t('linuxArtifacts.columns.pid'),
      className: 'w-[70px]',
      render: (row) => row.pid?.toString() ?? '-',
      text: (row) => row.pid?.toString() ?? '',
    },
    {
      key: 'message',
      title: t('linuxArtifacts.columns.message'),
      className: 'min-w-[240px]',
      render: (row) => row.message ?? '-',
      text: (row) => row.message ?? '',
    },
    {
      key: 'sourcePath',
      title: t('linuxArtifacts.columns.sourcePath'),
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath,
      text: (row) => row.sourcePath,
    },
  ];
}

export function buildLoginColumns(t: TFunction): DenseColumn<LinuxLoginRecord>[] {
  return [
    {
      key: 'recordKind',
      title: t('linuxArtifacts.columns.recordKind'),
      className: 'w-[100px]',
      filterable: true,
      render: (row) => row.recordKind ?? '-',
      text: (row) => row.recordKind ?? '',
    },
    {
      key: 'loginTime',
      title: t('linuxArtifacts.columns.loginTime'),
      className: 'w-[180px]',
      render: (row) => row.loginTime ?? '-',
      text: (row) => row.loginTime ?? '',
    },
    {
      key: 'logoutTime',
      title: t('linuxArtifacts.columns.logoutTime'),
      className: 'w-[180px]',
      render: (row) => row.logoutTime ?? '-',
      text: (row) => row.logoutTime ?? '',
    },
    {
      key: 'uid',
      title: t('linuxArtifacts.columns.uid'),
      className: 'w-[70px]',
      render: (row) => row.uid?.toString() ?? '-',
      text: (row) => row.uid?.toString() ?? '',
    },
    {
      key: 'user',
      title: t('linuxArtifacts.columns.user'),
      className: 'w-[120px]',
      render: (row) => row.user || '-',
      text: (row) => row.user,
    },
    {
      key: 'terminal',
      title: t('linuxArtifacts.columns.terminal'),
      className: 'w-[100px]',
      filterable: true,
      render: (row) => row.terminal || '-',
      text: (row) => row.terminal,
    },
    {
      key: 'host',
      title: t('linuxArtifacts.columns.host'),
      className: 'w-[140px]',
      render: (row) => row.host || '-',
      text: (row) => row.host,
    },
    {
      key: 'failures',
      title: t('linuxArtifacts.columns.failures'),
      className: 'w-[80px]',
      render: (row) => row.failures?.toString() ?? '-',
      text: (row) => row.failures?.toString() ?? '',
    },
    {
      key: 'lockout',
      title: t('linuxArtifacts.columns.lockout'),
      className: 'w-[80px]',
      render: (row) =>
        row.lockout === undefined ? '-' : row.lockout ? t('linuxArtifacts.values.yes') : t('linuxArtifacts.values.no'),
      text: (row) =>
        row.lockout === undefined ? '' : row.lockout ? t('linuxArtifacts.values.yes') : t('linuxArtifacts.values.no'),
    },
    {
      key: 'recordType',
      title: t('linuxArtifacts.columns.recordType'),
      className: 'w-[80px]',
      render: (row) =>
        row.recordKind === 'lastlog' || row.recordKind === 'faillog' ? '-' : (row.recordType?.toString() ?? '-'),
      text: (row) =>
        row.recordKind === 'lastlog' || row.recordKind === 'faillog' ? '' : (row.recordType?.toString() ?? ''),
    },
    {
      key: 'sourcePath',
      title: t('linuxArtifacts.columns.sourcePath'),
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath,
      text: (row) => row.sourcePath,
    },
  ];
}

export function buildCommandColumns(t: TFunction): DenseColumn<LinuxBashCommand>[] {
  return [
    {
      key: 'timestamp',
      title: t('linuxArtifacts.columns.timestamp'),
      className: 'w-[180px]',
      render: (row) => row.timestamp ?? '-',
      text: (row) => row.timestamp ?? '',
    },
    {
      key: 'command',
      title: t('linuxArtifacts.columns.command'),
      className: 'min-w-[300px]',
      render: (row) => <span className="font-mono">{row.command}</span>,
      text: (row) => row.command,
    },
    {
      key: 'lineNumber',
      title: t('linuxArtifacts.columns.lineNumber'),
      className: 'w-[80px]',
      render: (row) => row.lineNumber.toString(),
      text: (row) => row.lineNumber.toString(),
    },
    {
      key: 'sourcePath',
      title: t('linuxArtifacts.columns.sourcePath'),
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath,
      text: (row) => row.sourcePath,
    },
  ];
}
