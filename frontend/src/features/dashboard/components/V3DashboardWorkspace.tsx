
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
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
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';

interface V3DashboardWorkspaceProps {
  model: V3DashboardModel;
}

export function V3DashboardWorkspace({ model }: V3DashboardWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full w-full flex-1 overflow-hidden bg-forensics-base p-[2px]">
      <WorkspaceCard className="flex flex-1 flex-col">
      <header className="flex shrink-0 items-center justify-between bg-forensics-paper px-5 py-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-forensics-ink-3">{t('navigation.links.v3.label')}</div>
          <h1 className="mt-1 text-xl font-semibold text-forensics-ink">{t('navigation.links.v3.context')}</h1>
        </div>
        <Tooltip content={t('dashboard.refresh')}><Button aria-label={t('dashboard.refresh')} type="button" variant="forensicsGhost" size="iconSm" onClick={model.refresh} disabled={!model.hasCase || model.loading}><ActionIcon name="refresh" /></Button></Tooltip>
      </header>
      {!model.hasCase && model.currentCaseIsSuccess ? <AnalysisEmptyState /> : model.loading ? <AnalysisLoadingPanel text={t('dashboard.loading')} /> : (
        <ScrollArea className="min-h-0 flex-1" viewportClassName="workspace-scroll-viewport min-h-full space-y-3 p-3">
          {model.error ? <AnalysisErrorBanner message={errorMessage(model.error)} onRetry={model.refresh} /> : null}
          <GraphStatsSection data={model.graph.data} />
          <div className="grid gap-3 xl:grid-cols-2">
            <DataSourceCoverageSection dataSources={model.overview.data?.dataSources} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
            <TimelineOverviewSection total={model.overview.data?.timelineEventCount} isLoading={model.overview.isLoading} isError={model.overview.isError} isSuccess={model.overview.isSuccess} error={model.overview.error} />
            <ArtifactStatsSection data={model.overview.data?.artifactFamilyCounts} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
            <CorrelationStatsSection data={model.overview.data?.correlationStatistics} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
            <PlatformCoverageSection data={model.overview.data?.platformCoverage} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
            <RulePackStatusSection data={model.overview.data?.rulePackCoverage} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
            <BatchStatusSection data={model.overview.data?.batchStatus} isLoading={model.overview.isLoading} isError={model.overview.isError} error={model.overview.error} />
          </div>
        </ScrollArea>
      )}
      </WorkspaceCard>
    </div>
  );
}
