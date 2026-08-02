import { Globe, Layers, Monitor, Server } from 'lucide-react';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { PlatformCoverage } from '@/types/models';

export function PlatformCoverageSection({ data, isLoading, isError, error }: { data: PlatformCoverage | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  return (
    <section>
      <SectionHeader icon={Layers} title="平台覆盖" subtitle="痕迹家族按目标平台分布" />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      {data ? (
        <>
          <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
            <MetricCard label="Windows" value={data.windowsArtifactFamilies} subtitle="个家族" icon={Monitor} size="lg" />
            <MetricCard label="Linux" value={data.linuxArtifactFamilies} subtitle="个家族" icon={Server} size="lg" />
            <MetricCard label="跨平台" value={data.crossPlatformArtifactFamilies} subtitle="个家族" icon={Globe} size="lg" />
            <MetricCard label="未分类" value={data.unknownArtifactFamilies} subtitle="个家族" icon={Layers} size="lg" />
          </div>
          <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">家族明细</div>
            <div className="space-y-3">
              {data.windowsFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary">
                    <Monitor size={12} /> Windows
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.windowsFamilies.map((f) => (
                      <span key={f} className="rounded-none border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.crossPlatformFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary">
                    <Globe size={12} /> 跨平台
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.crossPlatformFamilies.map((f) => (
                      <span key={f} className="rounded-none border border-forensics-border bg-forensics-info-bg px-2 py-0.5 font-mono text-[10px] text-forensics-info-text">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.linuxFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary">
                    <Server size={12} /> Linux
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.linuxFamilies.map((f) => (
                      <span key={f} className="rounded-none border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.unknownFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary"><Layers size={12} /> 未分类</div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.unknownFamilies.map((f) => <span key={f} className="rounded-none border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">{f}</span>)}
                  </div>
                </div>
              )}
            </div>
          </div>
        </>
      ) : null}
      </DashboardQueryState>
    </section>
  );
}
