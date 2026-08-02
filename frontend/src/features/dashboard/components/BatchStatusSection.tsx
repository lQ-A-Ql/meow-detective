import { Activity, BarChart3, GitBranch, Layers, Shield } from 'lucide-react';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { BatchStatus } from '@/types/models';

export function BatchStatusSection({ data, isLoading, isError, error }: { data: BatchStatus | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  return (
    <section>
      <SectionHeader icon={Activity} title="批处理状态" subtitle="批量导入、批量提取、批量报告" />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      {data ? (
        <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-5">
          <MetricCard label="进行中" value={data.activeJobs} icon={Activity} size="lg" />
          <MetricCard label="已完成" value={data.completedJobs} icon={Shield} size="lg" />
          <MetricCard label="失败" value={data.failedJobs} icon={BarChart3} size="lg" />
          <MetricCard label="排队中" value={data.queuedJobs} icon={Layers} size="lg" />
          <MetricCard label="总计" value={data.totalJobs} icon={GitBranch} size="lg" />
        </div>
      ) : <div className="mt-3 rounded-none border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-lighter">暂无批处理作业。</div>}
      </DashboardQueryState>
    </section>
  );
}
