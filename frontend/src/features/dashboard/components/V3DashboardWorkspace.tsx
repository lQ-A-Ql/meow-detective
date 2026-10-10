import { RefreshCw } from 'lucide-react';
import { ContextBar } from '@/components/layout/ContextBar';
import { useTranslation } from 'react-i18next';
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
import { errorMessage } from '@/lib/errors';
import type { V3DashboardModel } from '@/features/dashboard/use-v3-dashboard-model';

interface V3DashboardWorkspaceProps {
  model: V3DashboardModel;
}

/** Pure dashboard presentation surface. Dashboard data loading belongs to the workspace model. */
export function V3DashboardWorkspace({ model }: V3DashboardWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <ContextBar align="end">
        <Button type="button" variant="outline" onClick={model.refresh} disabled={!model.hasCase || model.loading} className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"><RefreshCw size={13} /> {t('dashboard.refresh')}</Button>
      </ContextBar>
      {!model.hasCase && model.currentCaseIsSuccess ? <AnalysisEmptyState /> : model.loading ? <AnalysisLoadingPanel text={t('dashboard.loading')} /> : (
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
