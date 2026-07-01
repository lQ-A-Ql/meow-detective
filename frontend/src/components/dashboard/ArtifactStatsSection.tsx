import { BarChart3, Shield } from 'lucide-react';
import { StatCard, SectionHeader } from '@/app/pages/V3ScoreCards';
import type { FamilyCount } from '@/types/models';

export function ArtifactStatsSection({ data }: { data: FamilyCount[] | undefined }) {
  return (
    <section>
      <SectionHeader icon={Shield} title="痕迹统计" subtitle="Windows 痕迹家族枚举" />
      <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard title="家族数量" value={data?.length ?? 0} icon={Shield} />
        <StatCard title="记录总量" value={data?.reduce((sum, a) => sum + a.count, 0) ?? 0} icon={BarChart3} />
      </div>
      {data && data.length > 0 ? (
        <div className="mt-3 rounded border border-forensics-border bg-white p-4">
          <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">家族明细</div>
          <div className="flex flex-wrap gap-2">
            {data.map((a) => (
              <div
                key={a.family}
                className="flex items-center gap-1.5 rounded border border-forensics-border bg-forensics-panel px-2 py-1 text-xs"
              >
                <span className="font-mono text-forensics-text">{a.family}</span>
                <span className="font-mono text-forensics-muted-light">{a.count}</span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}
