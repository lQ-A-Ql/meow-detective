import { RefreshCw } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { AnalysisEmptyState, AnalysisErrorBanner, AnalysisLoadingPanel } from '@/features/analysis/components/AnalysisPanels';
import { ArtifactStatsSection } from '@/features/dashboard/components/ArtifactStatsSection';
import { BatchStatusSection } from '@/features/dashboard/components/BatchStatusSection';
import { CorrelationStatsSection } from '@/features/dashboard/components/CorrelationStatsSection';
import { DataSourceCoverageSection } from '@/features/dashboard/components/DataSourceCoverageSection';
import { GraphStatsSection } from '@/features/dashboard/components/GraphStatsSection';
import { PlatformCoverageSection } from '@/features/dashboard/components/PlatformCoverageSection';
import { RulePackStatusSection } from '@/features/dashboard/components/RulePackStatusSection';
import { TimelineOverviewSection } from '@/features/dashboard/components/TimelineOverviewSection';
import { errorMessage } from '@/features/dashboard/components/V3ScoreCards';
import type { V3DashboardModel } from '@/features/dashboard/use-v3-dashboard-model';

interface V3DashboardWorkspaceProps {
  model: V3DashboardModel;
}

/** Pure dashboard presentation surface. Dashboard data loading belongs to the workspace model. */
export function V3DashboardWorkspace({ model }: V3DashboardWorkspaceProps) {
  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div><div className="font-serif text-xl tracking-tight text-forensics-text">取证总览</div><div className="mt-1 font-mono text-[11px] text-forensics-muted">图统计 / 数据源覆盖 / 痕迹关联 / 规则包状态</div></div>
          <Button type="button" variant="outline" onClick={model.refresh} disabled={!model.hasCase || model.loading} className="h-8 rounded-none border-forensics-350 bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"><RefreshCw size={14} className={model.graph.isFetching || model.overview.isFetching ? 'opacity-70' : ''} />刷新</Button>
        </div>
      </div>
      {!model.hasCase && model.currentCaseIsSuccess ? <AnalysisEmptyState /> : model.loading ? <AnalysisLoadingPanel text="正在加载取证总览快照..." /> : (
        <ScrollArea className="min-h-0 flex-1" viewportClassName="space-y-6 p-6">
          {model.error ? <AnalysisErrorBanner message={errorMessage(model.error)} onRetry={model.refresh} /> : null}
          <GraphStatsSection data={model.graph.data} />
          <DataSourceCoverageSection dataSources={model.overview.data?.dataSources} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
          <TimelineOverviewSection total={model.overview.data?.timelineEventCount} isLoading={model.overview.isLoading} isError={model.overview.isError} isSuccess={model.overview.isSuccess} error={model.overview.error} />
          <ArtifactStatsSection data={model.overview.data?.artifactFamilyCounts} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
          <CorrelationStatsSection data={model.overview.data?.correlationStatistics} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
          <PlatformCoverageSection data={model.overview.data?.platformCoverage} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
          <RulePackStatusSection data={model.overview.data?.rulePackCoverage} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
          <BatchStatusSection data={model.overview.data?.batchStatus} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
        </ScrollArea>
      )}
    </div>
  );
}
