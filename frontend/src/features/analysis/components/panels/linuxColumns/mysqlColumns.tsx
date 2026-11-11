import type { TFunction } from 'i18next';
import type { LinuxMysqlConfig, LinuxMysqlFinding, LinuxMysqlLogEntry } from '@/types/models';
import type { DenseColumn } from '@/components/tables/DenseDataTable';

export function buildMysqlFindingColumns(t: TFunction): DenseColumn<LinuxMysqlFinding>[] {
  return [
    {
      key: 'severity',
      title: t('linuxArtifacts.columns.severity'),
      className: 'w-[90px]',
      filterable: true,
      render: (row) => row.severity,
      text: (row) => row.severity,
    },
    {
      key: 'findingKind',
      title: t('linuxArtifacts.columns.findingKind'),
      className: 'w-[170px]',
      render: (row) => row.findingKind,
      text: (row) => row.findingKind,
    },
    {
      key: 'confidence',
      title: t('linuxArtifacts.columns.confidence'),
      className: 'w-[90px]',
      render: (row) => `${Math.round(row.confidence * 100)}%`,
      text: (row) => `${Math.round(row.confidence * 100)}%`,
    },
    {
      key: 'evidence',
      title: t('linuxArtifacts.columns.evidence'),
      className: 'min-w-[300px]',
      render: (row) => row.evidence,
      text: (row) => row.evidence,
    },
    {
      key: 'sourcePath',
      title: t('linuxArtifacts.columns.sourcePath'),
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath,
      text: (row) => row.sourcePath,
    },
    {
      key: 'lineNumber',
      title: t('linuxArtifacts.columns.lineNumber'),
      className: 'w-[80px]',
      render: (row) => row.lineNumber.toString(),
      text: (row) => row.lineNumber.toString(),
    },
  ];
}

export function buildMysqlConfigColumns(t: TFunction): DenseColumn<LinuxMysqlConfig>[] {
  return [
    {
      key: 'section',
      title: t('linuxArtifacts.columns.section'),
      className: 'w-[120px]',
      render: (row) => row.section ?? '-',
      text: (row) => row.section ?? '',
    },
    {
      key: 'key',
      title: t('linuxArtifacts.columns.key'),
      className: 'w-[180px]',
      render: (row) => <span className="font-mono">{row.key}</span>,
      text: (row) => row.key,
    },
    {
      key: 'value',
      title: t('linuxArtifacts.columns.value'),
      className: 'min-w-[240px]',
      render: (row) => <span className="font-mono">{row.value}</span>,
      text: (row) => row.value,
    },
    {
      key: 'sourcePath',
      title: t('linuxArtifacts.columns.sourcePath'),
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath,
      text: (row) => row.sourcePath,
    },
    {
      key: 'lineNumber',
      title: t('linuxArtifacts.columns.lineNumber'),
      className: 'w-[80px]',
      render: (row) => row.lineNumber.toString(),
      text: (row) => row.lineNumber.toString(),
    },
  ];
}

export function buildMysqlLogColumns(t: TFunction): DenseColumn<LinuxMysqlLogEntry>[] {
  return [
    {
      key: 'timestamp',
      title: t('linuxArtifacts.columns.timestamp'),
      className: 'w-[180px]',
      render: (row) => row.timestamp ?? '-',
      text: (row) => row.timestamp ?? '',
    },
    {
      key: 'severity',
      title: t('linuxArtifacts.columns.severity'),
      className: 'w-[110px]',
      filterable: true,
      render: (row) => row.severity ?? '-',
      text: (row) => row.severity ?? '-',
    },
    {
      key: 'threadId',
      title: t('linuxArtifacts.columns.threadId'),
      className: 'w-[90px]',
      render: (row) => row.threadId ?? '-',
      text: (row) => row.threadId ?? '',
    },
    {
      key: 'message',
      title: t('linuxArtifacts.columns.message'),
      className: 'min-w-[320px]',
      render: (row) => row.message,
      text: (row) => row.message,
    },
    {
      key: 'sourcePath',
      title: t('linuxArtifacts.columns.sourcePath'),
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath,
      text: (row) => row.sourcePath,
    },
    {
      key: 'lineNumber',
      title: t('linuxArtifacts.columns.lineNumber'),
      className: 'w-[80px]',
      render: (row) => row.lineNumber.toString(),
      text: (row) => row.lineNumber.toString(),
    },
  ];
}
