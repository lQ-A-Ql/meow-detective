import { Globe, Layers, Monitor, Server } from 'lucide-react';
import { StatCard, SectionHeader } from '@/features/dashboard/components/V3ScoreCards';
import type { PlatformCoverage } from '@/types/models';

export function PlatformCoverageSection({ data }: { data: PlatformCoverage | undefined }) {
  return (
    <section>
      <SectionHeader icon={Layers} title="平台覆盖" subtitle="痕迹家族按目标平台分布" />
      {data ? (
        <>
          <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-3">
            <StatCard title="Windows" value={data.windowsArtifactFamilies} subtitle="个家族" icon={Monitor} />
            <StatCard title="Linux" value={data.linuxArtifactFamilies} subtitle="个家族" icon={Server} />
            <StatCard title="跨平台" value={data.crossPlatformArtifactFamilies} subtitle="个家族" icon={Globe} />
          </div>
          <div className="mt-3 rounded border border-forensics-border bg-white p-4">
            <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">家族明细</div>
            <div className="space-y-3">
              {data.windowsFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-forensics-text-tertiary">
                    <Monitor size={12} /> Windows
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.windowsFamilies.map((f) => (
                      <span key={f} className="rounded border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.crossPlatformFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-forensics-text-tertiary">
                    <Globe size={12} /> 跨平台
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.crossPlatformFamilies.map((f) => (
                      <span key={f} className="rounded border border-forensics-border bg-forensics-info-bg px-2 py-0.5 font-mono text-[10px] text-forensics-info-text">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.linuxFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-forensics-text-tertiary">
                    <Server size={12} /> Linux
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.linuxFamilies.map((f) => (
                      <span key={f} className="rounded border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        </>
      ) : (
        <div className="mt-3 rounded border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-lighter">
          暂无平台覆盖数据。导入数据源并运行痕迹提取后生成。
        </div>
      )}
    </section>
  );
}
