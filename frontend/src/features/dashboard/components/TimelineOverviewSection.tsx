import { Activity } from 'lucide-react';
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
  return (
    <section>
      <SectionHeader icon={Activity} title="时间线概览" subtitle="跨数据源事件总量与就绪状态" />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={isSuccess}>
        <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
          <MetricCard label="事件总量" value={total ?? 0} icon={Activity} size="lg" />
          <MetricCard label="时间线就绪" value="Yes" size="lg" />
          <MetricCard label="时间线状态" value="Ready" size="lg" />
        </div>
      </DashboardQueryState>
    </section>
  );
}
