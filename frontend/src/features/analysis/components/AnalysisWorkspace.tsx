import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import {
  useFileClassificationBoard,
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
import { subscribeToEvent } from '@/lib/events/subscribers';
import { refreshAnalysisQueries } from '@/features/analysis/refresh';
import { runSelectedSourceExtraction } from '@/features/analysis/extraction-runner';
import { useDeletedRecoveryModel } from '@/features/recovery/hooks';
import {
  AnalysisSourceEpoch,
  analysisSourceContextKey,
  type ExtractionCategory,
  type LinuxAnalysisTabKey,
  isExtractionCategory,
} from '@/features/analysis/types';
import {
  labeledProgress,
  useAnalysisStore,
} from '@/stores/analysis-store';
import { useUiStore } from '@/stores/ui-store';
import type { AnalysisExtractionProgress } from '@/types/models';
import type { EvtxEventView } from '@/types/models';

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
  const [analysisRefreshError, setAnalysisRefreshError] = useState<unknown>();
  const [eventLogView, setEventLogView] = useState<EvtxEventView>('boot');

  const systemInfo = useAnalysisSystemInfo(selectedDataSource);
  const evidenceSummary = useEvidenceClassificationSummary(selectedDataSource);
  const evidenceScan = useRunEvidenceClassification();
  const extractionRun = useRunAnalysisExtraction();
  const registrySummary = useRegistryExtractionSummary({ source: selectedDataSource, limit: 200 });
  const registryStructured = useRegistryStructuredSummary(selectedDataSource);
  const browserSummary = useBrowserHistorySummary({ source: selectedDataSource, limit: 200 });
  const emailSummary = useEmailExtractionSummary({ source: selectedDataSource, limit: 200 });
  const eventLogSummary = useEvtxEventSummary({
    source: selectedDataSource,
    view: eventLogView,
  });
  const linuxSummary = useLinuxArtifactSummary({ source: selectedDataSource, limit: 200 });
  const classificationBoard = useFileClassificationBoard(selectedDataSource, 300);
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
  const deletedRecovery = useDeletedRecoveryModel(
    selectedDataSource,
    (selectedPlatform === 'windows' && activeTab === 'deletedRecovery')
      || (selectedPlatform === 'linux' && activeLinuxTab === 'deletedRecovery'),
  );

  const analysisMutationPending = evidenceScan.isPending
    || extractionRun.isPending
    || summaryMutation.isPending
    || extractionRunning
    || deletedRecovery.scanning
    || deletedRecovery.reading
    || deletedRecovery.exporting;

  const labeledExtractionProgress = useMemo(
    () => labeledProgress(extractionProgress, t),
    [extractionProgress, t],
  );
  const linuxNodeCounts = useMemo<Partial<Record<LinuxAnalysisTabKey, number>>>(() => {
    const summary = selectedPlatform === 'linux' ? linuxSummary.data : undefined;
    const counts: Partial<Record<LinuxAnalysisTabKey, number>> = {};
    if (summary) {
      Object.assign(counts, {
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
      });
    }
    if (deletedRecovery.state === 'ready') {
      counts.deletedRecovery = deletedRecovery.total;
    }
    return counts;
  }, [deletedRecovery.state, deletedRecovery.total, linuxSummary.data, selectedPlatform]);

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
    ?? classificationBoard.error
    ?? summaryMutation.error
    ?? analysisRefreshError;
  const linuxError = currentCase.error
    ?? extractionRun.error
    ?? linuxSummary.error
    ?? analysisRefreshError;
  useLayoutEffect(() => {
    if (sourceEpoch.sync(selectedSourceContextKey)) {
      setAnalysisRefreshError(undefined);
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

  useEffect(() => {
    // Extraction progress events fire per candidate batch; applying each one
    // to the store re-renders the whole workspace at event frequency.
    // Coalesce per category (~150ms trailing) and apply terminal phases
    // immediately so completion is never delayed.
    const pending = new Map<ExtractionCategory, AnalysisExtractionProgress>();
    let flushTimer: ReturnType<typeof setTimeout> | undefined;
    const applyProgress = (
      category: ExtractionCategory,
      progress: AnalysisExtractionProgress,
    ) => {
      const completed = progress.phase === 'completed';
      const failed = progress.phase === 'failed';
      updateExtractionProgress(category, {
        status: failed ? 'failed' : completed ? 'success' : 'running',
        scannedCount: progress.processedCandidates,
        artifactCount: progress.artifactCount,
        timelineEventCount: progress.timelineEventCount,
        totalCandidateCount: progress.totalCandidates,
        processedCandidateCount: progress.processedCandidates,
        structuredCandidateCount: progress.structuredCandidates,
        unsupportedCandidateCount: progress.unsupportedCandidates,
        textFallbackCandidateCount: progress.textFallbackCandidates,
        warningCandidateCount: progress.warningCandidates,
        checkpointHitCount: progress.checkpointHitCount,
        phase: progress.phase,
        currentPath: progress.currentPath ?? undefined,
        detail: progress.detail,
        error: failed ? progress.detail : undefined,
      });
    };
    const flush = () => {
      flushTimer = undefined;
      pending.forEach((progress, category) => applyProgress(category, progress));
      pending.clear();
    };
    const unsubscribe = subscribeToEvent<AnalysisExtractionProgress>('analysis-extraction-progress', (event) => {
      const progress = event.payload;
      const category = progress.category;
      if (
        progress.caseId !== currentCase.data?.id
        || progress.dataSourceId !== selectedDataSource?.id
        || !isExtractionCategory(category)
      ) {
        return;
      }
      if (progress.phase === 'completed' || progress.phase === 'failed') {
        if (flushTimer !== undefined) {
          clearTimeout(flushTimer);
        }
        flush();
        applyProgress(category, progress);
        return;
      }
      pending.set(category, progress);
      flushTimer ??= setTimeout(flush, 150);
    });
    return () => {
      if (flushTimer !== undefined) {
        clearTimeout(flushTimer);
      }
      unsubscribe();
    };
  }, [
    currentCase.data?.id,
    selectedDataSource?.id,
    updateExtractionProgress,
  ]);

  async function refresh() {
    try {
      await refreshAnalysisQueries(
        selectedPlatform,
        [
          systemInfo.refetch,
          evidenceSummary.refetch,
          registrySummary.refetch,
          registryStructured.refetch,
          browserSummary.refetch,
          emailSummary.refetch,
          eventLogSummary.refetch,
          classificationBoard.refetch,
        ],
        linuxSummary.refetch,
      );
      setAnalysisRefreshError(undefined);
    } catch (error) {
      setAnalysisRefreshError(error);
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
    } finally {
      sourceEpoch.finish(operation);
    }
  }

  async function runExtraction() {
    await runSelectedSourceExtraction({
      source: selectedDataSource,
      sourceContextKey: selectedSourceContextKey,
      sourceEpoch,
      execute: extractionRun.mutateAsync,
      updateProgress: updateExtractionProgress,
      resetProgress: resetExtractionProgress,
      setExtractionRunning,
      setDrawerOpen,
      setRefreshError: setAnalysisRefreshError,
      setActiveOperation: (operation) => {
        extractionOperationRef.current = operation;
      },
      isActiveOperation: (operation) => extractionOperationRef.current === operation,
    });
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
            eventLogView={eventLogView}
            onEventLogViewChange={setEventLogView}
            classificationBoard={classificationBoard}
            evidencePending={evidenceScan.isPending}
            onRunEvidence={runEvidenceScan}
             summaryPending={summaryMutation.isPending}
             onDownloadSummary={downloadSummary}
             recoveryModel={deletedRecovery}
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
            extractionRunning={extractionRunning}
            recoveryModel={deletedRecovery}
          />
        ) : null}
      </div>
    </div>
  );
}
