import { useTranslation } from 'react-i18next';
import type { DataSourceSummary } from '@/types/models';

export function useLinuxRootMountModel(source: DataSourceSummary) {
  const { t } = useTranslation();
  const mounts = source.linuxRootMounts ?? [];
  return {
    visible: source.platform === 'linux',
    hint: t(mounts.length ? 'caseHome.dataSources.linuxRootHint' : 'caseHome.dataSources.linuxRootUnknown'),
    entries: mounts.map((mount) => ({
      id: `${source.id}-${mount.fstabPartitionIndex}-${mount.evidenceLine}`,
      text: t('caseHome.dataSources.linuxRootMount', {
        status: t(`caseHome.dataSources.rootMountStatus.${mount.status}`, { defaultValue: t('caseHome.dataSources.rootMountStatus.unresolved') }),
        source: mount.sourceSpec,
        evidence: `${mount.evidencePath}:${mount.evidenceLine}`,
        partition: mount.resolvedPartitionIndex === undefined
          ? t('caseHome.dataSources.rootPartitionUnknown')
          : t('caseHome.dataSources.rootPartition', { index: mount.resolvedPartitionIndex }),
        evidencePartition: mount.fstabPartitionIndex,
        uuid: mount.filesystemUuid ?? t('caseHome.partitions.uuidNotRecorded'),
      }),
    })),
  };
}
