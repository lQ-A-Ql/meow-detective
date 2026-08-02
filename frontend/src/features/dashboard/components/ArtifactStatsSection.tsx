import { BarChart3, Shield } from 'lucide-react';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { FamilyCount } from '@/types/models';

export function ArtifactStatsSection({
  data,
  isLoading,
  isError,
  error,
}: {
  data: FamilyCount[] | undefined;
  isLoading?: boolean;
  isError?: boolean;
  error?: unknown;
}) {
  return (
    <section>
      <SectionHeader icon={Shield} title="痕迹统计" subtitle="按痕迹家族汇总" />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
        <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
          <MetricCard label="家族数量" value={data?.length ?? 0} icon={Shield} size="lg" />
          <MetricCard label="记录总量" value={data?.reduce((sum, a) => sum + a.count, 0) ?? 0} icon={BarChart3} size="lg" />
        </div>
        {data && data.length > 0 ? (
          <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">家族明细</div>
            <div className="flex flex-wrap gap-2">
              {data.map((a) => (
                <div key={a.family} className="flex items-center gap-1.5 rounded-none border border-forensics-border bg-forensics-panel px-2 py-1 text-xs">
                  <span className="font-mono text-forensics-text">{a.family}</span>
                  <span className="font-mono text-forensics-muted-light">{a.count}</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </DashboardQueryState>
    </section>
  );
}
