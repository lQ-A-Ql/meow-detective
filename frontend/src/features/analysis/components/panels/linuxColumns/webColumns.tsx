import type { TFunction } from 'i18next';
import type {
  LinuxSystemConfig,
  LinuxWebAccessLog,
  LinuxWebErrorLog,
  LinuxWebFinding,
  LinuxWebSite,
} from '@/types/models';
import type { DenseColumn } from '@/components/tables/DenseDataTable';

export function buildSystemConfigColumns(t: TFunction): DenseColumn<LinuxSystemConfig>[] {
  return [
    {
      key: 'configKind',
      title: t('linuxArtifacts.columns.configKind'),
      className: 'w-[130px]',
      filterable: true,
      render: (row) => row.configKind || '-',
      text: (row) => row.configKind || '-',
    },
    {
      key: 'key',
      title: t('linuxArtifacts.columns.key'),
      className: 'w-[140px]',
      render: (row) => row.key ?? row.username ?? '-',
      text: (row) => row.key ?? row.username ?? '',
    },
    {
      key: 'value',
      title: t('linuxArtifacts.columns.value'),
      className: 'min-w-[220px]',
      render: (row) => row.value ?? row.line ?? '-',
      text: (row) => row.value ?? row.line ?? '',
    },
    {
      key: 'uid',
      title: t('linuxArtifacts.columns.uid'),
      className: 'w-[70px]',
      render: (row) => row.uid?.toString() ?? '-',
      text: (row) => row.uid?.toString() ?? '',
    },
    {
      key: 'gid',
      title: t('linuxArtifacts.columns.gid'),
      className: 'w-[70px]',
      render: (row) => row.gid?.toString() ?? '-',
      text: (row) => row.gid?.toString() ?? '',
    },
    {
      key: 'home',
      title: t('linuxArtifacts.columns.home'),
      className: 'w-[160px]',
      render: (row) => row.home ?? '-',
      text: (row) => row.home ?? '',
    },
    {
      key: 'shell',
      title: t('linuxArtifacts.columns.shell'),
      className: 'w-[140px]',
      render: (row) => row.shell ?? '-',
      text: (row) => row.shell ?? '',
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

export function buildWebSiteColumns(t: TFunction): DenseColumn<LinuxWebSite>[] {
  return [
    {
      key: 'serverKind',
      title: t('linuxArtifacts.columns.serverKind'),
      className: 'w-[90px]',
      render: (row) => row.serverKind,
      text: (row) => row.serverKind,
    },
    {
      key: 'siteName',
      title: t('linuxArtifacts.columns.siteName'),
      className: 'min-w-[160px]',
      render: (row) => row.siteName,
      text: (row) => row.siteName,
    },
    {
      key: 'hostnames',
      title: t('linuxArtifacts.columns.hostnames'),
      className: 'min-w-[180px]',
      render: (row) => row.hostnames.join(', ') || '-',
      text: (row) => row.hostnames.join(', '),
    },
    {
      key: 'listen',
      title: t('linuxArtifacts.columns.listen'),
      className: 'w-[140px]',
      render: (row) => row.listen.join(', ') || '-',
      text: (row) => row.listen.join(', '),
    },
    {
      key: 'documentRoots',
      title: t('linuxArtifacts.columns.documentRoots'),
      className: 'min-w-[220px]',
      render: (row) => row.documentRoots.join(', ') || '-',
      text: (row) => row.documentRoots.join(', '),
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

export function buildWebAccessColumns(t: TFunction): DenseColumn<LinuxWebAccessLog>[] {
  return [
    {
      key: 'timestamp',
      title: t('linuxArtifacts.columns.timestamp'),
      className: 'w-[180px]',
      render: (row) => row.timestamp ?? '-',
      text: (row) => row.timestamp ?? '',
    },
    {
      key: 'clientIp',
      title: t('linuxArtifacts.columns.clientIp'),
      className: 'w-[130px]',
      render: (row) => row.clientIp,
      text: (row) => row.clientIp,
    },
    {
      key: 'method',
      title: t('linuxArtifacts.columns.method'),
      className: 'w-[70px]',
      filterable: true,
      render: (row) => row.method,
      text: (row) => row.method,
    },
    {
      key: 'status',
      title: t('linuxArtifacts.columns.statusCode'),
      className: 'w-[80px]',
      filterable: true,
      render: (row) => row.status.toString(),
      text: (row) => row.status.toString(),
    },
    {
      key: 'uri',
      title: t('linuxArtifacts.columns.uri'),
      className: 'min-w-[260px]',
      render: (row) => <span className="font-mono">{row.uri}</span>,
      text: (row) => row.uri,
    },
    {
      key: 'userAgent',
      title: t('linuxArtifacts.columns.userAgent'),
      className: 'min-w-[200px]',
      render: (row) => row.userAgent ?? '-',
      text: (row) => row.userAgent ?? '',
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

export function buildWebErrorColumns(t: TFunction): DenseColumn<LinuxWebErrorLog>[] {
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
  ];
}

export function buildWebFindingColumns(t: TFunction): DenseColumn<LinuxWebFinding>[] {
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
      className: 'w-[150px]',
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
      key: 'clientIp',
      title: t('linuxArtifacts.columns.clientIp'),
      className: 'w-[130px]',
      render: (row) => row.clientIp ?? '-',
      text: (row) => row.clientIp ?? '',
    },
    {
      key: 'uri',
      title: t('linuxArtifacts.columns.uri'),
      className: 'min-w-[220px]',
      render: (row) => row.uri ?? '-',
      text: (row) => row.uri ?? '',
    },
    {
      key: 'evidence',
      title: t('linuxArtifacts.columns.evidence'),
      className: 'min-w-[260px]',
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
  ];
}
