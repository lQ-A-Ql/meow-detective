import { RefreshCw } from 'lucide-react';
import { AnalysisEmptyState, AnalysisErrorBanner, AnalysisLoadingPanel } from '@/features/analysis/components/AnalysisPanels';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';
import { useTimelineEvents } from '@/features/timeline/hooks';
import { useArtifactFamilyCounts } from '@/features/artifacts/hooks';
import { useCorrelationSnapshot, useV3GovernanceSnapshot } from '@/features/analysis/hooks';
import { Button } from '@/app/components/ui/button';
import { errorMessage } from '@/features/dashboard/components/V3ScoreCards';
import { GraphStatsSection } from '@/features/dashboard/components/GraphStatsSection';
import { DataSourceCoverageSection } from '@/features/dashboard/components/DataSourceCoverageSection';
import { TimelineOverviewSection } from '@/features/dashboard/components/TimelineOverviewSection';
import { ArtifactStatsSection } from '@/features/dashboard/components/ArtifactStatsSection';
import { CorrelationStatsSection } from '@/features/dashboard/components/CorrelationStatsSection';
import { PlatformCoverageSection } from '@/features/dashboard/components/PlatformCoverageSection';
import { RulePackStatusSection } from '@/features/dashboard/components/RulePackStatusSection';
import { BatchStatusSection } from '@/features/dashboard/components/BatchStatusSection';

export function V3Dashboard() {
  const currentCase = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const caseId = currentCase.data?.id ?? '';
  const graph = useGraphSnapshot(caseId);
  const timeline = useTimelineEvents({ limit: 1 });
  const artifactCounts = useArtifactFamilyCounts();
  const correlation = useCorrelationSnapshot();
  const v3Governance = useV3GovernanceSnapshot();

  const hasCase = Boolean(currentCase.data);
  const loading =
    currentCase.isLoading || graph.isLoading || timeline.isLoading || artifactCounts.isLoading || correlation.isLoading || v3Governance.isLoading;
  const error = currentCase.error ?? graph.error ?? timeline.error ?? artifactCounts.error ?? correlation.error ?? v3Governance.error;

  async function refresh() {
    await Promise.all([
      graph.refetch(),
      timeline.refetch(),
      artifactCounts.refetch(),
      correlation.refetch(),
      v3Governance.refetch(),
    ]);
  }

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-white">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="font-serif text-xl tracking-tight text-forensics-text">取证总览</div>
            <div className="mt-1 font-mono text-[11px] text-forensics-muted">
              图统计 / 数据源覆盖 / 痕迹关联 / 规则包状态
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={refresh}
              disabled={!hasCase || loading}
              className="h-8 rounded border-forensics-350 bg-white px-3 text-[12px] hover:bg-forensics-panel-strong"
            >
              <RefreshCw size={14} className={graph.isFetching || timeline.isFetching ? 'animate-spin' : ''} />
              刷新
            </Button>
          </div>
        </div>
      </div>

      {!hasCase && currentCase.isSuccess ? (
        <AnalysisEmptyState />
      ) : loading ? (
        <AnalysisLoadingPanel text="正在加载取证总览快照..." />
      ) : (
        <div className="flex-1 space-y-6 overflow-auto p-6">
          {error ? <AnalysisErrorBanner message={errorMessage(error)} onRetry={refresh} /> : null}

          <GraphStatsSection data={graph.data} />
          <DataSourceCoverageSection dataSources={dataSources} />
          <TimelineOverviewSection
            total={timeline.data?.total}
            isLoading={timeline.isLoading}
            isError={timeline.isError}
            isSuccess={timeline.isSuccess}
          />
          <ArtifactStatsSection data={artifactCounts.data} />
          <CorrelationStatsSection data={correlation.data} />
          <PlatformCoverageSection data={v3Governance.data?.platformCoverage} />
          <RulePackStatusSection data={v3Governance.data?.rulePackCoverage} />
          <BatchStatusSection data={v3Governance.data?.batchStatus} />
        </div>
      )}
    </div>
  );
}
