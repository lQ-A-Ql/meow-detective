import { Activity } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';

export function TimelineOverviewSection({
  total,
  isLoading,
  isError,
  isSuccess,
  error,
}: {
  total: number | undefined;
  isLoading: boolean;
  isError: boolean;
  isSuccess: boolean;
  error?: unknown;
}) {
  const { t } = useTranslation();
  return (
    <section>
      <SectionHeader icon={Activity} title={t('dashboard.timeline.title')} subtitle={t('dashboard.timeline.subtitle')} />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={isSuccess}>
        <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
          <MetricCard label={t('dashboard.timeline.total')} value={total ?? 0} icon={Activity} size="lg" />
          <MetricCard label={t('dashboard.timeline.ready')} value={t('common.yes')} size="lg" />
          <MetricCard label={t('dashboard.timeline.status')} value={t('dashboard.status.ready')} size="lg" />
        </div>
      </DashboardQueryState>
    </section>
  );
}
