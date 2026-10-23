import { Activity, BarChart3, GitBranch, Layers, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { GraphVisualizationContainer } from '@/features/graph/containers/GraphVisualizationContainer';
import { MetricCard, StatGrid } from '@/components/data-display';
import type { GraphSnapshot } from '@/types/models';

export function GraphStatsSection({ data }: { data: GraphSnapshot | undefined }) {
  const { t } = useTranslation();
  return (
    <section>
      <StatGrid
        collapseZeros
        hiddenNote={(hidden) => t('dashboard.metricsHidden', { count: hidden })}
        className="mt-3 grid-cols-2 gap-3 md:grid-cols-3 lg:grid-cols-5"
      >
        {data ? (
          <>
            <MetricCard label={t('dashboard.graph.nodes')} value={data.totalNodes} icon={GitBranch} size="lg" />
            <MetricCard label={t('dashboard.graph.edges')} value={data.totalEdges} icon={Activity} size="lg" />
            <MetricCard label={t('dashboard.graph.density')} value={data.density} icon={Layers} size="lg" />
            <MetricCard
              label={t('dashboard.graph.largestComponent')}
              value={data.largestComponentSize || t('dashboard.graph.notCalculated')}
              icon={BarChart3}
              size="lg"
            />
            <MetricCard label={t('dashboard.graph.nodeTypes')} value={Object.keys(data.nodeCountByType ?? {}).length} icon={Shield} size="lg" />
          </>
        ) : null}
      </StatGrid>
      {data && data.projectionStatus !== 'ready' && data.projectionStatus !== 'source' ? (
        <div
          role="status"
          className="mt-3 border border-forensics-warning-border bg-forensics-warning-bg px-3 py-2 text-[11px] text-forensics-warning-text"
        >
          {t(`dashboard.graph.projection.${data.projectionStatus ?? 'missing'}`)}
        </div>
      ) : null}
      {data ? (
        <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <div className="mb-1.5 font-mono text-[10px] uppercase tracking-wider text-forensics-muted-light">{t('dashboard.graph.byNodeType')}</div>
            <div className="flex flex-wrap gap-2">
              {Object.entries(data.nodeCountByType ?? {}).map(([type, count]) => (
                <div key={type} className="rounded-none bg-transparent px-2 py-1 text-[11px]">
                  <span className="font-mono text-forensics-text">{type}</span>
                  <span className="ml-1.5 font-mono text-forensics-muted-light">{count}</span>
                </div>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1.5 font-mono text-[10px] uppercase tracking-wider text-forensics-muted-light">{t('dashboard.graph.byEdgeType')}</div>
            <div className="flex flex-wrap gap-2">
              {Object.entries(data.edgeCountByType ?? {}).map(([type, count]) => (
                <div key={type} className="rounded-none bg-transparent px-2 py-1 text-[11px]">
                  <span className="font-mono text-forensics-text">{type}</span>
                  <span className="ml-1.5 font-mono text-forensics-muted-light">{count}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}

      <div className="mt-3">
      <GraphVisualizationContainer />
      </div>
    </section>
  );
}
