import { errorMessage } from '@/lib/errors';
import { AnalysisEmptyState, AnalysisHeader } from '@/features/analysis/components/AnalysisPanels';
import { AnalysisSourceSidebar } from '@/features/analysis/components/AnalysisSourceSidebar';
import { LinuxAnalysisView } from '@/features/analysis/components/LinuxAnalysisView';
import { WindowsAnalysisView } from '@/features/analysis/components/WindowsAnalysisView';
import type { AnalysisWorkspaceModel } from '@/features/analysis/use-analysis-workspace-model';

interface AnalysisWorkspaceProps {
  model: AnalysisWorkspaceModel;
}

/** Pure analysis presentation surface. Runtime orchestration belongs to the feature model. */
export function AnalysisWorkspace({ model }: AnalysisWorkspaceProps) {
  return (
    <div className="flex h-full w-full flex-1 overflow-hidden bg-forensics-surface">
      {model.hasCase ? (
        <AnalysisSourceSidebar
          dataSources={model.readyDataSources}
          selectedDataSourceId={model.selectedDataSourceId}
          disabled={model.analysisMutationPending}
          progress={model.labeledExtractionProgress}
          linuxNodeCounts={model.linuxNodeCounts}
          activeWindowsTab={model.activeTab}
          activeLinuxTab={model.activeLinuxTab}
          onSelectDataSource={model.selectDataSource}
          onWindowsTabChange={model.setActiveTab}
          onLinuxTabChange={model.setActiveLinuxTab}
        />
      ) : null}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <AnalysisHeader
          loading={model.loading}
          hasCase={model.hasCase}
          extractionPending={model.extractionPending}
          onRefresh={model.refresh}
          onRunExtraction={model.runExtraction}
          selectedDataSourceId={model.selectedDataSourceId}
        />

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
            evidencePending={model.evidencePending}
            onRunEvidence={model.runEvidenceScan}
            summaryPending={model.summaryPending}
            onDownloadSummary={model.downloadSummary}
            recoveryModel={model.recoveryModel}
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
          />
        ) : null}
      </div>
    </div>
  );
}
