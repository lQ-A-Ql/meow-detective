import { Activity } from 'lucide-react';
import { StatCard, SectionHeader } from '@/features/dashboard/components/V3ScoreCards';

export function TimelineOverviewSection({
  total,
  isLoading,
  isError,
  isSuccess,
}: {
  total: number | undefined;
  isLoading: boolean;
  isError: boolean;
  isSuccess: boolean;
}) {
  return (
    <section>
      <SectionHeader icon={Activity} title="时间线概览" subtitle="事件总量与类型分布" />
      <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard title="事件总量" value={total ?? 0} icon={Activity} />
        <StatCard title="时间线就绪" value={isSuccess ? 'Yes' : 'No'} />
        <StatCard title="时间线状态" value={isLoading ? 'Loading' : isError ? 'Error' : 'Ready'} />
      </div>
    </section>
  );
}
