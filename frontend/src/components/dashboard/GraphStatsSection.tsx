import { Activity, BarChart3, GitBranch, Layers, Shield } from 'lucide-react';
import { GraphVisualizationSection } from '@/components/graph/GraphVisualizationSection';
import { StatCard, SectionHeader } from '@/app/pages/V3ScoreCards';
import type { GraphSnapshot } from '@/types/models';

export function GraphStatsSection({ data }: { data: GraphSnapshot | undefined }) {
  return (
    <section>
      <SectionHeader icon={GitBranch} title="图统计" subtitle="节点与边按类型分布" />
      <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-3 lg:grid-cols-5">
        {data ? (
          <>
            <StatCard title="节点总数" value={data.totalNodes} icon={GitBranch} />
            <StatCard title="边总数" value={data.totalEdges} icon={Activity} />
            <StatCard title="密度" value={data.density} icon={Layers} />
            <StatCard title="最大联通分量" value={data.largestComponentSize} icon={BarChart3} />
            <StatCard title="节点类型数" value={Object.keys(data.nodeCountByType).length} icon={Shield} />
          </>
        ) : null}
      </div>
      {data ? (
        <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <div className="mb-1.5 font-mono text-[10px] uppercase tracking-wider text-forensics-muted-light">按节点类型</div>
            <div className="flex flex-wrap gap-2">
              {Object.entries(data.nodeCountByType).map(([type, count]) => (
                <div key={type} className="rounded border border-forensics-border bg-forensics-panel px-2 py-1 text-[11px]">
                  <span className="font-mono text-forensics-text">{type}</span>
                  <span className="ml-1.5 font-mono text-forensics-muted-light">{count}</span>
                </div>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1.5 font-mono text-[10px] uppercase tracking-wider text-forensics-muted-light">按边类型</div>
            <div className="flex flex-wrap gap-2">
              {Object.entries(data.edgeCountByType).map(([type, count]) => (
                <div key={type} className="rounded border border-forensics-border bg-forensics-panel px-2 py-1 text-[11px]">
                  <span className="font-mono text-forensics-text">{type}</span>
                  <span className="ml-1.5 font-mono text-forensics-muted-light">{count}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}

      <div className="mt-3">
        <GraphVisualizationSection />
      </div>
    </section>
  );
}
