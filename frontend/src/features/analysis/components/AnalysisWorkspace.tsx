import { errorMessage } from '@/lib/errors';
import { useTranslation } from 'react-i18next';
import { LoaderCircle, CircleAlert } from '@/components/icons';
import { AnalysisEmptyState, AnalysisHeader } from '@/features/analysis/components/AnalysisPanels';
import { AnalysisSourceSidebar } from '@/features/analysis/components/AnalysisSourceSidebar';
import { AndroidAnalysisView } from '@/features/analysis/components/AndroidAnalysisView';
import { LinuxAnalysisView } from '@/features/analysis/components/LinuxAnalysisView';
import { WindowsAnalysisView } from '@/features/analysis/components/WindowsAnalysisView';
import type { AnalysisWorkspaceModel } from '@/features/analysis/use-analysis-workspace-model';
import type { CapabilityProgress } from '@/types/models';
import { WorkspaceFrame } from '@/components/layout/WorkspaceFrame';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';

function formatArtifactProgress(progress?: CapabilityProgress): string | undefined {
  if (!progress) return undefined;
  const total = progress.total;
  const done = progress.done;
  const phase = progress.phase ? ` · ${progress.phase}` : '';
  return total !== undefined ? `${done}/${total}${phase}` : `${done}${phase}`;
}

/** Lightweight advisory strip: extraction in progress / partial / deferred / failed never blocks viewing. */
function AnalysisCapabilityHint({
  artifactsState,
  artifactsProgress,
  artifactsDetail,
}: {
  artifactsState: string;
  artifactsProgress?: CapabilityProgress;
  artifactsDetail?: string;
}) {
  const { t } = useTranslation();
  const progressText = formatArtifactProgress(artifactsProgress);
  if (artifactsState === 'running' || artifactsState === 'queued') {
    return (
      <div className="flex items-center gap-2 border-b border-forensics-warning-border bg-forensics-warning-bg px-4 py-2 text-[11px] text-forensics-warning-text">
        <LoaderCircle size={12} className="shrink-0 animate-spin" />
        <span className="min-w-0 flex-1">
          {progressText
            ? t('analysis.capability.artifactsRunningWithProgress', { progress: progressText })
            : t('analysis.capability.artifactsRunning')}
        </span>
      </div>
    );
  }
  if (artifactsState === 'partial') {
    return (
      <div className="flex items-center gap-2 border-b border-forensics-border bg-forensics-panel px-4 py-2 text-[11px] text-forensics-muted">
        <CircleAlert size={12} className="shrink-0" />
        <span className="min-w-0 flex-1">{t('analysis.capability.artifactsPartial')}</span>
      </div>
    );
  }
  if (artifactsState === 'deferred') {
    return (
      <div className="flex items-center gap-2 border-b border-forensics-warning-border bg-forensics-warning-bg px-4 py-2 text-[11px] text-forensics-warning-text">
        <CircleAlert size={12} className="shrink-0" />
        <span className="min-w-0 flex-1">
          {t('analysis.capability.artifactsDeferred')}
          {artifactsDetail ? `: ${artifactsDetail}` : ''}
        </span>
      </div>
    );
  }
  if (artifactsState === 'failed') {
    return (
      <div className="flex items-center gap-2 border-b border-forensics-error-border bg-forensics-error-bg px-4 py-2 text-[11px] text-forensics-error-text">
        <CircleAlert size={12} className="shrink-0" />
        <span className="min-w-0 flex-1">
          {t('analysis.capability.artifactsFailed')}
          {artifactsDetail ? `: ${artifactsDetail}` : ''}
        </span>
      </div>
    );
  }
  return null;
}

interface AnalysisWorkspaceProps {
  model: AnalysisWorkspaceModel;
}

/** Pure analysis presentation surface. Runtime orchestration belongs to the feature model. */
export function AnalysisWorkspace({ model }: AnalysisWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <WorkspaceFrame>
      {model.hasCase ? (
        <WorkspaceCard className="flex w-52 shrink-0" aria-label={t('analysis.sidebar.treeLabel')}>
          <AnalysisSourceSidebar
            dataSources={model.readyDataSources}
            selectedDataSourceId={model.selectedDataSourceId}
            disabled={model.analysisMutationPending}
            progress={model.labeledExtractionProgress}
            windowsNodeCounts={model.windowsNodeCounts}
            linuxNodeCounts={model.linuxNodeCounts}
            activeWindowsTab={model.activeTab}
            activeLinuxTab={model.activeLinuxTab}
            activeAndroidTab={model.activeAndroidTab}
            onSelectDataSource={model.selectDataSource}
            onWindowsTabChange={model.setActiveTab}
            onLinuxTabChange={model.setActiveLinuxTab}
            onAndroidTabChange={model.setActiveAndroidTab}
            pluginModules={model.pluginModules}
            activePluginId={model.activePluginId}
            onSelectPluginModule={model.selectPluginModule}
          />
        </WorkspaceCard>
      ) : null}
      <WorkspaceCard className="flex flex-1 flex-col" aria-label={t('analysis.sidebar.treeLabel')}>
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <AnalysisHeader
            loading={model.loading}
            hasCase={model.hasCase}
            onRefresh={model.refresh}
            selectedDataSourceId={model.selectedDataSourceId}
          />
          {model.artifactsCapabilityState && model.selectedPlatform ? (
            <AnalysisCapabilityHint
              artifactsState={model.artifactsCapabilityState}
              artifactsProgress={model.artifactsCapabilityProgress}
              artifactsDetail={model.artifactsCapabilityDetail}
            />
          ) : null}

          {!model.hasCase && model.currentCaseIsSuccess ? (
            <AnalysisEmptyState />
          ) : model.selectedPlatform === 'windows' ? (
            <WindowsAnalysisView
              activeTab={model.activeTab}
              onActiveTabChange={model.setActiveTab}
              error={model.windowsError ? errorMessage(model.windowsError) : undefined}
              onRetry={model.refresh}
              loading={model.loading}
              systemInfo={model.systemInfo}
              evidenceSummary={model.evidenceSummary}
              registrySummary={model.registrySummary}
              registryStructured={model.registryStructured}
              browserSummary={model.browserSummary}
              emailSummary={model.emailSummary}
              eventLogSummary={model.eventLogSummary}
              eventLogView={model.eventLogView}
              eventLogLoadContextKey={model.eventLogLoadContextKey}
              onEventLogViewChange={model.setEventLogView}
              classificationBoard={model.classificationBoard}
              summaryPending={model.summaryPending}
              onDownloadSummary={model.downloadSummary}
              recoveryModel={model.recoveryModel}
              pluginModules={model.pluginModules}
              activePluginId={model.activePluginId}
              dataSourceId={model.selectedDataSourceId}
            />
          ) : model.selectedPlatform === 'linux' ? (
            <LinuxAnalysisView
              activeTab={model.activeLinuxTab}
              onActiveTabChange={model.setActiveLinuxTab}
              error={model.linuxError ? errorMessage(model.linuxError) : undefined}
              onRetry={model.refresh}
              loading={model.loading}
              summary={model.linuxSummary}
              summaryLoading={model.linuxSummaryLoading}
              extractionRunning={model.extractionRunning}
              recoveryModel={model.recoveryModel}
              hasMore={model.linuxSummaryHasMore}
              loadingMore={model.linuxSummaryLoadingMore}
              loadMoreFailed={model.linuxSummaryLoadMoreFailed}
              loadStateKey={model.linuxSummaryLoadStateKey}
              loadContextKey={model.linuxLoadContextKey}
              onLoadMore={model.loadMoreLinuxSummary}
              onRetryLoadMore={model.retryLinuxSummaryLoad}
              pluginModules={model.pluginModules}
              activePluginId={model.activePluginId}
              dataSourceId={model.selectedDataSourceId}
            />
          ) : model.selectedPlatform === 'android' ? (
            <AndroidAnalysisView
              deviceInfo={model.androidDeviceInfo}
              packageSummary={model.androidPackages}
              activePanel={model.activeAndroidTab}
              loading={model.loading || model.androidLoading}
              error={model.androidError ? errorMessage(model.androidError) : undefined}
              hasMore={model.androidPackagesHasMore}
              loadingMore={model.androidPackagesLoadingMore}
              onRetry={model.refresh}
              onLoadMore={model.loadMoreAndroidPackages}
            />
          ) : null}
        </div>
      </WorkspaceCard>
    </WorkspaceFrame>
  );
}
