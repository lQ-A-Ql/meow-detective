import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { LinuxAccount, LinuxArtifactSummary } from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { EmptyLine, InfoCard, TableBlock } from './helpers';

/** Overview tab: derived host identity (os-release/hostname), kernels, and account stats. */
export function LinuxOverviewSection({ info }: { info: LinuxArtifactSummary }) {
  const { t } = useTranslation();
  const systemInfo = info.systemInfo;

  const accountColumns = useMemo<DenseColumn<LinuxAccount>[]>(() => {
    const yesNo = (value?: boolean) =>
      value === undefined ? '-' : t(value ? 'linuxArtifacts.values.yes' : 'linuxArtifacts.values.no');
    return [
      { key: 'username', title: t('linuxArtifacts.columns.username'), className: 'min-w-[140px]', render: (row) => row.username, text: (row) => row.username },
      { key: 'uid', title: t('linuxArtifacts.columns.uid'), className: 'w-[70px]', render: (row) => row.uid?.toString() ?? '-', text: (row) => row.uid?.toString() ?? '' },
      { key: 'gid', title: t('linuxArtifacts.columns.gid'), className: 'w-[70px]', render: (row) => row.gid?.toString() ?? '-', text: (row) => row.gid?.toString() ?? '' },
      { key: 'home', title: t('linuxArtifacts.columns.home'), className: 'min-w-[160px]', render: (row) => row.home ?? '-', text: (row) => row.home ?? '' },
      { key: 'shell', title: t('linuxArtifacts.columns.shell'), className: 'min-w-[140px]', filterable: true, render: (row) => row.shell ?? '-', text: (row) => row.shell ?? '-' },
      { key: 'locked', title: t('linuxArtifacts.columns.locked'), className: 'w-[80px]', filterable: true, render: (row) => yesNo(row.locked), text: (row) => yesNo(row.locked) },
      { key: 'hasPassword', title: t('linuxArtifacts.columns.hasPassword'), className: 'w-[90px]', filterable: true, render: (row) => yesNo(row.hasPassword), text: (row) => yesNo(row.hasPassword) },
    ];
  }, [t]);

  if (!systemInfo) {
    return info.totalCount === 0 ? (
      <EmptyLine text={t('linuxArtifacts.empty.overview.description')} />
    ) : (
      <EmptyLine text={t('linuxArtifacts.overview.unavailable')} />
    );
  }

  const accounts = systemInfo.accounts ?? [];

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-3 xl:grid-cols-4">
        <InfoCard label={t('linuxArtifacts.overview.os')} value={systemInfo.osPrettyName} />
        <InfoCard label={t('linuxArtifacts.overview.osId')} value={systemInfo.osId} />
        <InfoCard label={t('linuxArtifacts.overview.osVersion')} value={systemInfo.osVersionId} />
        <InfoCard label={t('linuxArtifacts.overview.hostname')} value={systemInfo.hostname} />
        <InfoCard
          label={t('linuxArtifacts.overview.kernelVersions')}
          value={systemInfo.kernelVersions?.join('、') || undefined}
        />
        <InfoCard
          label={t('linuxArtifacts.overview.accountCount')}
          value={systemInfo.accountCount.toString()}
        />
        <InfoCard
          label={t('linuxArtifacts.overview.userAccountCount')}
          value={systemInfo.userAccountCount.toString()}
        />
        <InfoCard
          label={t('linuxArtifacts.overview.lockedAccountCount')}
          value={systemInfo.lockedAccountCount.toString()}
        />
      </div>
      {accounts.length > 0 ? (
        <TableBlock title={t('linuxArtifacts.overview.accountsTitle')}>
          <DenseDataTableFrame rowCount={accounts.length}>
            <DenseDataTable
              rows={accounts}
              columns={accountColumns}
              getRowKey={(account) => account.username}
              emptyTitle={t('linuxArtifacts.overview.accountsTitle')}
              emptyDescription={t('linuxArtifacts.overview.unavailable')}
              filterable
            />
          </DenseDataTableFrame>
        </TableBlock>
      ) : null}
    </div>
  );
}
