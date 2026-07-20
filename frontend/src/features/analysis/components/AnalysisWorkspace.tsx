import { useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useBrowserHistorySummary,
  useEmailExtractionSummary,
  useEvtxEventSummary,
  useEvidenceClassificationSummary,
  useGenerateAnalysisSummary,
  useLinuxArtifactSummary,
  useRegistryExtractionSummary,
  useRegistryStructuredSummary,
  useRunAnalysisExtraction,
  useRunEvidenceClassification,
} from '@/features/analysis/hooks';
import {
  AnalysisEmptyState,
  AnalysisHeader,
} from '@/features/analysis/components/AnalysisPanels';
import { AnalysisSourceSidebar } from '@/features/analysis/components/AnalysisSourceSidebar';
import { LinuxAnalysisView } from '@/features/analysis/components/LinuxAnalysisView';
import { WindowsAnalysisView } from '@/features/analysis/components/WindowsAnalysisView';
import { errorMessage } from '@/lib/errors';
import {
  AnalysisSourceEpoch,
  EXTRACTION_CATEGORIES_BY_PLATFORM,
  LINUX_PROGRESS_CATEGORIES,
  analysisSourceContextKey,
  type ExtractionCategory,
  type LinuxAnalysisTabKey,
  isExtractionCategory,
} from '@/features/analysis/types';
import {
  labeledProgress,
  statusFromRun,
  useAnalysisStore,
} from '@/stores/analysis-store';
import { useUiStore } from '@/stores/ui-store';

export function AnalysisWorkspace() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const selectedDataSourceId = useAnalysisStore((s) => s.selectedDataSourceId);
  const setSelectedDataSourceId = useAnalysisStore((s) => s.setSelectedDataSourceId);
  const readyDataSources = useMemo(
    () => (dataSources ?? []).filter(
      (source) => source.importState === 'ready'
        && (source.platform === 'windows' || source.platform === 'linux'),
    ),
    [dataSources],
  );
  const selectedDataSource = useMemo(
    () => readyDataSources.find((source) => source.id === selectedDataSourceId),
    [readyDataSources, selectedDataSourceId],
  );
  const selectedPlatform = selectedDataSource?.platform;
  const selectedSourceContextKey = analysisSourceContextKey(
    currentCase.data?.id,
    selectedDataSource,
  );
  const sourceEpoch = useRef(new AnalysisSourceEpoch(selectedSourceContextKey)).current;
  const extractionOperationRef = useRef<ReturnType<AnalysisSourceEpoch['begin']>>();

  const systemInfo = useAnalysisSystemInfo(selectedDataSource);
  const evidenceSummary = useEvidenceClassificationSummary(selectedDataSource);
  const evidenceScan = useRunEvidenceClassification();
  const extractionRun = useRunAnalysisExtraction();
  const registrySummary = useRegistryExtractionSummary({ source: selectedDataSource, limit: 200 });
  const registryStructured = useRegistryStructuredSummary(selectedDataSource);
  const browserSummary = useBrowserHistorySummary({ source: selectedDataSource, limit: 200 });
  const emailSummary = useEmailExtractionSummary({ source: selectedDataSource, limit: 200 });
  const eventLogSummary = useEvtxEventSummary({ source: selectedDataSource, limit: 200 });
  const linuxSummary = useLinuxArtifactSummary({ source: selectedDataSource, limit: 200 });
  const classifications = useAnalysisClassifications(selectedDataSource, 1000);
  const summaryMutation = useGenerateAnalysisSummary(selectedDataSource?.id);
  const resetEvidenceScan = evidenceScan.reset;
  const resetExtractionRun = extractionRun.reset;
  const resetSummaryMutation = summaryMutation.reset;

  const extractionProgress = useAnalysisStore((s) => s.extractionProgress);
  const extractionRunning = useAnalysisStore((s) => s.extractionRunning);
  const activeTab = useAnalysisStore((s) => s.activeTab);
  const activeLinuxTab = useAnalysisStore((s) => s.activeLinuxTab);
  const updateExtractionProgress = useAnalysisStore((s) => s.updateExtractionProgress);
  const resetExtractionProgress = useAnalysisStore((s) => s.resetExtractionProgress);
  const setExtractionRunning = useAnalysisStore((s) => s.setExtractionRunning);
  const setActiveTab = useAnalysisStore((s) => s.setActiveTab);
  const setActiveLinuxTab = useAnalysisStore((s) => s.setActiveLinuxTab);
  const setDrawerOpen = useUiStore((state) => state.setDrawerOpen);

  const analysisMutationPending = evidenceScan.isPending
    || extractionRun.isPending
    || summaryMutation.isPending
    || extractionRunning;

  const labeledExtractionProgress = useMemo(
    () => labeledProgress(extractionProgress, t),
    [extractionProgress, t],
  );
  const linuxNodeCounts = useMemo<Partial<Record<LinuxAnalysisTabKey, number>>>(() => {
    const summary = selectedPlatform === 'linux' ? linuxSummary.data : undefined;
    if (!summary) {
      return {};
    }

    return {
      overview: summary.totalCount,
      journal: summary.journalCount,
      login: summary.loginCount,
      commands: summary.bashCommandCount,
      packages: summary.aptEventCount,
      cron: summary.cronJobCount,
      sudo: summary.sudoEventCount,
      systemConfig: summary.systemConfigCount,
      webServices: (summary.webSiteCount ?? 0)
        + (summary.webAccessLogCount ?? 0)
        + (summary.webErrorLogCount ?? 0)
        + (summary.webFindingCount ?? 0),
      mysqlServices: (summary.mysqlConfigCount ?? 0)
        + (summary.mysqlLogCount ?? 0)
        + (summary.mysqlFindingCount ?? 0),
    };
  }, [linuxSummary.data, selectedPlatform]);

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading;
  const windowsError = currentCase.error
    ?? evidenceScan.error
    ?? extractionRun.error
    ?? systemInfo.error
    ?? evidenceSummary.error
    ?? registrySummary.error
    ?? browserSummary.error
    ?? emailSummary.error
    ?? eventLogSummary.error
    ?? classifications.error
    ?? summaryMutation.error;
  const linuxError = currentCase.error
    ?? extractionRun.error
    ?? linuxSummary.error;
  useLayoutEffect(() => {
    if (sourceEpoch.sync(selectedSourceContextKey)) {
      resetExtractionProgress();
      resetEvidenceScan();
      resetExtractionRun();
      resetSummaryMutation();
      if (extractionOperationRef.current) {
        extractionOperationRef.current = undefined;
        setExtractionRunning(false);
      }
    }
  }, [
    resetEvidenceScan,
    resetExtractionProgress,
    resetExtractionRun,
    resetSummaryMutation,
    selectedSourceContextKey,
    setExtractionRunning,
    sourceEpoch,
  ]);

  useEffect(() => {
    if (!readyDataSources.length) {
      if (selectedDataSourceId) {
        sourceEpoch.sync(undefined);
        resetExtractionProgress();
        resetEvidenceScan();
        resetExtractionRun();
        resetSummaryMutation();
        setSelectedDataSourceId(undefined);
      }
      return;
    }

    if (!selectedDataSource) {
      const nextSource = readyDataSources[0];
      const nextContextKey = analysisSourceContextKey(currentCase.data?.id, nextSource);
      sourceEpoch.sync(nextContextKey);
      resetExtractionProgress();
      resetEvidenceScan();
      resetExtractionRun();
      resetSummaryMutation();
      setSelectedDataSourceId(nextSource.id);
    }
  }, [
    currentCase.data?.id,
    readyDataSources,
    resetEvidenceScan,
    resetExtractionProgress,
    resetExtractionRun,
    resetSummaryMutation,
    selectedDataSource,
    selectedDataSourceId,
    sourceEpoch,
    setSelectedDataSourceId,
  ]);

  useEffect(() => () => {
    sourceEpoch.sync(undefined);
    if (extractionOperationRef.current) {
      extractionOperationRef.current = undefined;
      setExtractionRunning(false);
    }
  }, [setExtractionRunning, sourceEpoch]);

  async function refresh() {
    if (selectedPlatform === 'windows') {
      await Promise.all([
        systemInfo.refetch(),
        evidenceSummary.refetch(),
        registrySummary.refetch(),
        registryStructured.refetch(),
        browserSummary.refetch(),
        emailSummary.refetch(),
        eventLogSummary.refetch(),
        classifications.refetch(),
      ]);
      return;
    }

    if (selectedPlatform === 'linux') {
      await linuxSummary.refetch();
    }
  }

  async function runEvidenceScan() {
    if (!selectedDataSource || selectedDataSource.platform !== 'windows') {
      return;
    }
    const operation = sourceEpoch.begin(selectedSourceContextKey);
    if (!operation) {
      return;
    }
    try {
      await evidenceScan.mutateAsync({ dataSourceId: selectedDataSource.id, categories: [] });
      if (sourceEpoch.isCurrent(operation)) {
        await evidenceSummary.refetch();
      }
    } finally {
      sourceEpoch.finish(operation);
    }
  }

  async function runExtraction() {
    if (!selectedDataSource) {
      return;
    }
    const operation = sourceEpoch.begin(selectedSourceContextKey);
    if (!operation) {
      return;
    }
    const source = selectedDataSource;
    const categories = EXTRACTION_CATEGORIES_BY_PLATFORM[source.platform];
    extractionOperationRef.current = operation;
    setExtractionRunning(true);
    setDrawerOpen(true);
    resetExtractionProgress();
    const refetchByCategory: Record<ExtractionCategory, () => Promise<unknown>> = {
      Registry: registrySummary.refetch,
      BrowserHistory: browserSummary.refetch,
      Email: emailSummary.refetch,
      EventLogs: eventLogSummary.refetch,
      LinuxArtifacts: linuxSummary.refetch,
      LinuxJournal: linuxSummary.refetch,
      LinuxLogin: linuxSummary.refetch,
      LinuxCommands: linuxSummary.refetch,
      LinuxPackages: linuxSummary.refetch,
      LinuxCron: linuxSummary.refetch,
      LinuxSudo: linuxSummary.refetch,
      LinuxSystemConfig: linuxSummary.refetch,
      LinuxWebServices: linuxSummary.refetch,
      LinuxMysqlServices: linuxSummary.refetch,
    };

    const refetchRegistryStructured = async () => {
      await registryStructured.refetch();
    };

    try {
      for (const category of categories) {
        if (!sourceEpoch.isCurrent(operation)) {
          return;
        }
        const pendingCategories = category === 'LinuxArtifacts' ? LINUX_PROGRESS_CATEGORIES : [category];
        for (const progressCategory of pendingCategories) {
          updateExtractionProgress(progressCategory, {
            status: 'running',
            warnings: [],
            error: undefined,
          });
        }
        updateExtractionProgress(category, {
          status: 'running',
          warnings: [],
          error: undefined,
        });

        try {
          const run = await extractionRun.mutateAsync({
            dataSourceId: source.id,
            categories: [category],
          });
          if (!sourceEpoch.isCurrent(operation)) {
            return;
          }
          let hasRequestedSection = false;
          let sectionArtifactCount = 0;
          for (const section of run.sections ?? []) {
            if (!isExtractionCategory(section.key)) {
              continue;
            }
            hasRequestedSection ||= section.key === category;
            sectionArtifactCount += section.artifactCount;
            updateExtractionProgress(section.key, {
              status: statusFromRun(section.status),
              scannedCount: section.scannedCount,
              artifactCount: section.artifactCount,
              timelineEventCount: section.timelineEventCount,
              warnings: section.warnings,
              error: undefined,
            });
          }
          if (!hasRequestedSection) {
            updateExtractionProgress(category, {
              status: statusFromRun(run.status),
              scannedCount: run.scannedCount,
              artifactCount: sectionArtifactCount,
              timelineEventCount: run.timelineEventCount,
              warnings: run.warnings,
              error: undefined,
            });
          }
          if (!sourceEpoch.isCurrent(operation)) {
            return;
          }
          await refetchByCategory[category]();
          if (category === 'Registry' && sourceEpoch.isCurrent(operation)) {
            await refetchRegistryStructured();
          }
        } catch (err) {
          if (!sourceEpoch.isCurrent(operation)) {
            return;
          }
          updateExtractionProgress(category, {
            status: 'failed',
            error: errorMessage(err),
          });
          if (category === 'LinuxArtifacts') {
            for (const progressCategory of LINUX_PROGRESS_CATEGORIES) {
              updateExtractionProgress(progressCategory, {
                status: 'failed',
                error: errorMessage(err),
              });
            }
          }
        }
      }

      if (source.platform === 'windows' && sourceEpoch.isCurrent(operation)) {
        await evidenceSummary.refetch();
      }
    } finally {
      sourceEpoch.finish(operation);
      if (extractionOperationRef.current === operation) {
        extractionOperationRef.current = undefined;
        setExtractionRunning(false);
      }
    }
  }

  function selectDataSource(id: string) {
    if (
      id === selectedDataSourceId
      || analysisMutationPending
      || sourceEpoch.isBusy
    ) {
      return;
    }
    const nextSource = readyDataSources.find((source) => source.id === id);
    if (!nextSource) {
      return;
    }
    const nextContextKey = analysisSourceContextKey(currentCase.data?.id, nextSource);
    sourceEpoch.sync(nextContextKey);
    resetExtractionProgress();
    resetEvidenceScan();
    resetExtractionRun();
    resetSummaryMutation();
    setSelectedDataSourceId(id);
  }

  async function downloadSummary() {
    if (!selectedDataSource || selectedDataSource.platform !== 'windows') {
      return;
    }
    const operation = sourceEpoch.begin(selectedSourceContextKey);
    if (!operation) {
      return;
    }
    try {
      const summary = await summaryMutation.mutateAsync();
      if (!sourceEpoch.isCurrent(operation)) {
        return;
      }
      const blob = new Blob([summary], { type: 'text/markdown;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = 'analysis-report.md';
      link.click();
      URL.revokeObjectURL(url);
    } finally {
      sourceEpoch.finish(operation);
    }
  }

  return (
    <div className="flex h-full w-full flex-1 overflow-hidden bg-forensics-surface">
      {hasCase ? (
        <AnalysisSourceSidebar
          dataSources={readyDataSources}
          selectedDataSourceId={selectedDataSourceId}
          disabled={analysisMutationPending}
          progress={labeledExtractionProgress}
          linuxNodeCounts={linuxNodeCounts}
          activeWindowsTab={activeTab}
          activeLinuxTab={activeLinuxTab}
          onSelectDataSource={selectDataSource}
          onWindowsTabChange={setActiveTab}
          onLinuxTabChange={setActiveLinuxTab}
        />
      ) : null}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <AnalysisHeader
          loading={loading}
          hasCase={hasCase}
          extractionPending={extractionRun.isPending || extractionRunning}
          onRefresh={refresh}
          onRunExtraction={runExtraction}
          selectedDataSourceId={selectedDataSourceId}
        />

        {!hasCase && currentCase.isSuccess ? (
          <AnalysisEmptyState />
        ) : selectedPlatform === 'windows' ? (
          <WindowsAnalysisView
            activeTab={activeTab}
            onActiveTabChange={setActiveTab}
            error={windowsError ? errorMessage(windowsError) : undefined}
            onRetry={refresh}
            loading={loading}
            systemInfo={systemInfo}
            evidenceSummary={evidenceSummary}
            registrySummary={registrySummary}
            registryStructured={registryStructured.data}
            browserSummary={browserSummary}
            emailSummary={emailSummary}
            eventLogSummary={eventLogSummary}
            classifications={classifications}
            evidencePending={evidenceScan.isPending}
            onRunEvidence={runEvidenceScan}
            summaryPending={summaryMutation.isPending}
            onDownloadSummary={downloadSummary}
          />
        ) : selectedPlatform === 'linux' ? (
          <LinuxAnalysisView
            activeTab={activeLinuxTab}
            onActiveTabChange={setActiveLinuxTab}
            error={linuxError ? errorMessage(linuxError) : undefined}
            onRetry={refresh}
            loading={loading}
            summary={linuxSummary.data}
            summaryLoading={linuxSummary.isLoading}
          />
        ) : null}
      </div>
    </div>
  );
}
