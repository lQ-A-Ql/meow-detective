import { Activity, BarChart3, GitBranch, Layers, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { BatchStatus } from '@/types/models';

export function BatchStatusSection({ data, isLoading, isError, error }: { data: BatchStatus | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  const { t } = useTranslation();
  return (
    <section>
      <SectionHeader icon={Activity} title={t('dashboard.batch.title')} subtitle={t('dashboard.batch.subtitle')} />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      {data ? (
        <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-5">
          <MetricCard label={t('dashboard.batch.active')} value={data.activeJobs} icon={Activity} size="lg" />
          <MetricCard label={t('dashboard.batch.completed')} value={data.completedJobs} icon={Shield} size="lg" />
          <MetricCard label={t('dashboard.batch.failed')} value={data.failedJobs} icon={BarChart3} size="lg" />
          <MetricCard label={t('dashboard.batch.queued')} value={data.queuedJobs} icon={Layers} size="lg" />
          <MetricCard label={t('dashboard.batch.total')} value={data.totalJobs} icon={GitBranch} size="lg" />
        </div>
      ) : <div className="mt-3 rounded-none border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-lighter">{t('dashboard.batch.empty')}</div>}
      </DashboardQueryState>
    </section>
  );
}
