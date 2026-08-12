import { useTranslation } from 'react-i18next';
import type { LinuxArtifactSummary } from '@/types/models';
import { EmptyLine, InfoCard } from './helpers';

/** Overview tab: derived host identity (os-release/hostname) and account stats. */
export function LinuxOverviewSection({ info }: { info: LinuxArtifactSummary }) {
  const { t } = useTranslation();
  const systemInfo = info.systemInfo;

  if (!systemInfo) {
    return info.totalCount === 0 ? (
      <EmptyLine text={t('linuxArtifacts.empty.overview.description')} />
    ) : (
      <EmptyLine text={t('linuxArtifacts.overview.unavailable')} />
    );
  }

  return (
    <div className="grid grid-cols-2 gap-2 md:grid-cols-3 xl:grid-cols-4">
      <InfoCard label={t('linuxArtifacts.overview.os')} value={systemInfo.osPrettyName} />
      <InfoCard label={t('linuxArtifacts.overview.osId')} value={systemInfo.osId} />
      <InfoCard label={t('linuxArtifacts.overview.osVersion')} value={systemInfo.osVersionId} />
      <InfoCard label={t('linuxArtifacts.overview.hostname')} value={systemInfo.hostname} />
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
  );
}
