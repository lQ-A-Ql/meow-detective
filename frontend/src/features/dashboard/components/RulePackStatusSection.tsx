import { Activity, BarChart3, Shield } from 'lucide-react';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { RulePackStatus } from '@/types/models';

export function RulePackStatusSection({ data, isLoading, isError, error }: { data: RulePackStatus | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  return (
    <section>
      <SectionHeader icon={Shield} title="规则包状态" subtitle="规则版本、覆盖率、执行状态" />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      {data ? (
        <>
          <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
            <MetricCard label="已加载规则包" value={data.loadedPacks.length} icon={Shield} size="lg" />
            <MetricCard label="规则总数" value={data.totalRuleCount} icon={BarChart3} size="lg" />
            <MetricCard label="定义状态" value={data.loadStatus} icon={Activity} size="lg" />
            <MetricCard label="本案执行" value={data.executionStatus} icon={Activity} size="lg" />
          </div>
          {data.loadedPacks.length > 0 && (
            <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface p-4">
              <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">已加载规则包</div>
              <div className="space-y-1">
                {data.loadedPacks.map((pack) => (
                  <div key={pack.name} className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-forensics-text">{pack.name}</span>
                      <span className="font-mono text-forensics-500">v{pack.version}</span>
                    </div>
                    <div className="flex items-center gap-3 font-mono text-forensics-muted">
                      <span>{pack.ruleCount} 规则</span>
                      <span className="text-forensics-muted-light">{pack.author}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </>
      ) : <div className="mt-3 rounded-none border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-lighter">规则包数据将在导入数据源后加载。</div>}
      </DashboardQueryState>
    </section>
  );
}
