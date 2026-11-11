import type { TFunction } from 'i18next';
import type { LinuxAptEvent, LinuxCronJob, LinuxSudoEvent } from '@/types/models';
import type { DenseColumn } from '@/components/tables/DenseDataTable';

export function buildPackageColumns(t: TFunction): DenseColumn<LinuxAptEvent>[] {
  return [
    {
      key: 'timestamp',
      title: t('linuxArtifacts.columns.timestamp'),
      className: 'w-[180px]',
      render: (row) => row.timestamp ?? '-',
      text: (row) => row.timestamp ?? '',
    },
    {
      key: 'action',
      title: t('linuxArtifacts.columns.action'),
      className: 'w-[100px]',
      filterable: true,
      render: (row) => row.action,
      text: (row) => row.action,
    },
    {
      key: 'package',
      title: t('linuxArtifacts.columns.package'),
      className: 'min-w-[200px]',
      render: (row) => row.package,
      text: (row) => row.package,
    },
    {
      key: 'version',
      title: t('linuxArtifacts.columns.version'),
      className: 'w-[160px]',
      render: (row) => row.version,
      text: (row) => row.version,
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

export function buildCronColumns(t: TFunction): DenseColumn<LinuxCronJob>[] {
  return [
    {
      key: 'schedule',
      title: t('linuxArtifacts.columns.schedule'),
      className: 'w-[140px]',
      render: (row) => <span className="font-mono">{row.schedule}</span>,
      text: (row) => row.schedule,
    },
    {
      key: 'user',
      title: t('linuxArtifacts.columns.user'),
      className: 'w-[100px]',
      filterable: true,
      render: (row) => row.user ?? '-',
      text: (row) => row.user ?? '-',
    },
    {
      key: 'command',
      title: t('linuxArtifacts.columns.command'),
      className: 'min-w-[260px]',
      render: (row) => <span className="font-mono">{row.command}</span>,
      text: (row) => row.command,
    },
    {
      key: 'sourceFile',
      title: t('linuxArtifacts.columns.sourceFile'),
      className: 'min-w-[180px]',
      render: (row) => row.sourceFile,
      text: (row) => row.sourceFile,
    },
  ];
}

export function buildSudoColumns(t: TFunction): DenseColumn<LinuxSudoEvent>[] {
  return [
    {
      key: 'timestamp',
      title: t('linuxArtifacts.columns.timestamp'),
      className: 'w-[180px]',
      render: (row) => row.timestamp ?? '-',
      text: (row) => row.timestamp ?? '',
    },
    {
      key: 'user',
      title: t('linuxArtifacts.columns.user'),
      className: 'w-[100px]',
      render: (row) => row.user,
      text: (row) => row.user,
    },
    {
      key: 'targetUser',
      title: t('linuxArtifacts.columns.targetUser'),
      className: 'w-[100px]',
      render: (row) => row.targetUser ?? '-',
      text: (row) => row.targetUser ?? '',
    },
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
      text: (row) => (row.success ? t('linuxArtifacts.values.yes') : t('linuxArtifacts.values.no')),
    },
    {
      key: 'terminal',
      title: t('linuxArtifacts.columns.terminal'),
      className: 'w-[90px]',
      render: (row) => row.terminal ?? '-',
      text: (row) => row.terminal ?? '',
    },
    {
      key: 'workingDirectory',
      title: t('linuxArtifacts.columns.workingDirectory'),
      className: 'w-[160px]',
      render: (row) => row.workingDirectory ?? '-',
      text: (row) => row.workingDirectory ?? '',
    },
    {
      key: 'command',
      title: t('linuxArtifacts.columns.command'),
      className: 'min-w-[240px]',
      render: (row) => <span className="font-mono">{row.command}</span>,
      text: (row) => row.command,
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
