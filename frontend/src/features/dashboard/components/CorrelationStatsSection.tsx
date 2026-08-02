import { Activity, BarChart3, GitBranch, Shield } from 'lucide-react';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { CorrelationOverview } from '@/types/models';

export function CorrelationStatsSection({ data, isLoading, isError, error }: { data: CorrelationOverview | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  return (
    <section>
      <SectionHeader icon={BarChart3} title="关联统计" subtitle="关联分析快照" />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricCard label="关联节点" value={data?.nodeCount ?? 0} icon={GitBranch} size="lg" />
        <MetricCard label="关联边" value={data?.edgeCount ?? 0} icon={Activity} size="lg" />
        <MetricCard label="聚合簇" value={data?.clusterCount ?? 0} icon={Shield} size="lg" />
        <MetricCard label="线索数" value={data?.leadCount ?? 0} icon={Shield} size="lg" />
      </div>
      {data?.familyCoverage && data.familyCoverage.length > 0 ? (
        <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface p-4">
          <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">家族覆盖</div>
          <div className="space-y-1">
            {data.familyCoverage.map((fc, i) => (
              <div key={fc.family ?? i} className="flex items-center justify-between text-xs">
                <div className="flex items-center gap-2">
                  <span className="font-mono text-forensics-text">{fc.displayName}</span>
                  <span
                    className={`rounded-none px-1.5 py-0.5 text-[10px] font-light ${
                      fc.status === 'covered'
                        ? 'bg-forensics-badge-covered-bg text-forensics-badge-covered-text'
                        : fc.status === 'review'
                          ? 'bg-forensics-badge-review-bg text-forensics-badge-review-text'
                          : fc.status === 'missing'
                            ? 'bg-forensics-error-bg text-forensics-error-text'
                            : 'bg-forensics-hover text-forensics-muted'
                    }`}
                  >
                    {fc.status}
                  </span>
                </div>
                <div className="flex items-center gap-3 font-mono text-forensics-muted">
                  <span>{fc.leadCount} 线索</span>
                  <span>{fc.clusterCount} 簇</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}
      </DashboardQueryState>
    </section>
  );
}
