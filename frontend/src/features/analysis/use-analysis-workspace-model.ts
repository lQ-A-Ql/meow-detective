import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useFileClassificationBoard,
  useAnalysisSystemInfo,
  useAndroidDeviceInfo,
  useAndroidPackageSummary,
  useBrowserHistorySummary,
  useEmailExtractionSummary,
  useEvtxEventSummary,
  useEvidenceClassificationSummary,
  useGenerateAnalysisSummary,
  useLinuxArtifactSummary,
  usePluginModules,
  useRegistryExtractionSummary,
  useRegistryStructuredSummary,
} from '@/features/analysis/hooks';
import { refreshAnalysisQueries } from '@/features/analysis/refresh';
import {
  AnalysisSourceEpoch,
  analysisSourceContextKey,
  type ExtractionCategory,
  type LinuxAnalysisTabKey,
  isExtractionCategory,
} from '@/features/analysis/types';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useDeletedRecoveryModel } from '@/features/recovery/hooks';
import { subscribeToEvent } from '@/lib/events/subscribers';
import { labeledProgress, useAnalysisStore } from '@/stores/analysis-store';
import type { AnalysisExtractionProgress, EvtxEventView } from '@/types/models';

export function useAnalysisWorkspaceModel() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const selectedDataSourceId = useAnalysisStore((state) => state.selectedDataSourceId);
  const setSelectedDataSourceId = useAnalysisStore((state) => state.setSelectedDataSourceId);
  const readyDataSources = useMemo(
    () =>
      (dataSources ?? []).filter(
        (source) =>
          source.importState === 'ready' &&
          (source.platform === 'windows' || source.platform === 'linux' || source.platform === 'android'),
      ),
    [dataSources],
  );
  const selectedDataSource = useMemo(
    () => readyDataSources.find((source) => source.id === selectedDataSourceId),
    [readyDataSources, selectedDataSourceId],
  );
  const selectedPlatform = selectedDataSource?.platform;
  const selectedSourceContextKey = analysisSourceContextKey(currentCase.data?.id, selectedDataSource);
  const sourceEpoch = useRef(new AnalysisSourceEpoch(selectedSourceContextKey)).current;
  const [analysisRefreshError, setAnalysisRefreshError] = useState<unknown>();
  const [eventLogView, setEventLogView] = useState<EvtxEventView>('boot');

  const systemInfo = useAnalysisSystemInfo(selectedDataSource);
  const evidenceSummary = useEvidenceClassificationSummary(selectedDataSource);
  const registrySummary = useRegistryExtractionSummary({ source: selectedDataSource, limit: 200 });
  const registryStructured = useRegistryStructuredSummary(selectedDataSource);
  const browserSummary = useBrowserHistorySummary({ source: selectedDataSource, limit: 200 });
  const emailSummary = useEmailExtractionSummary({ source: selectedDataSource, limit: 200 });
  const eventLogSummary = useEvtxEventSummary({
    source: selectedDataSource,
    view: eventLogView,
  });
  const linuxSummary = useLinuxArtifactSummary({ source: selectedDataSource, limit: 200 });
  const androidDeviceInfo = useAndroidDeviceInfo(selectedDataSource);
  const androidPackages = useAndroidPackageSummary({ source: selectedDataSource, limit: 100 });
  const pluginModulesQuery = usePluginModules(selectedDataSource);
  const classificationBoard = useFileClassificationBoard(selectedDataSource, 300);
  const summaryMutation = useGenerateAnalysisSummary(selectedDataSource?.id);
  const resetSummaryMutation = summaryMutation.reset;

  const extractionProgress = useAnalysisStore((state) => state.extractionProgress);
  const extractionRunning = useAnalysisStore((state) => state.extractionRunning);
  const activeTab = useAnalysisStore((state) => state.activeTab);
  const activeLinuxTab = useAnalysisStore((state) => state.activeLinuxTab);
  const activeAndroidTab = useAnalysisStore((state) => state.activeAndroidTab);
  const updateExtractionProgress = useAnalysisStore((state) => state.updateExtractionProgress);
  const resetExtractionProgress = useAnalysisStore((state) => state.resetExtractionProgress);
  const setExtractionRunning = useAnalysisStore((state) => state.setExtractionRunning);
  const setActiveTab = useAnalysisStore((state) => state.setActiveTab);
  const setActiveLinuxTab = useAnalysisStore((state) => state.setActiveLinuxTab);
  const setActiveAndroidTab = useAnalysisStore((state) => state.setActiveAndroidTab);
  const activePluginId = useAnalysisStore((state) => state.activePluginId);
  const setActivePluginId = useAnalysisStore((state) => state.setActivePluginId);
  const deletedRecovery = useDeletedRecoveryModel(
    selectedDataSource,
    (selectedPlatform === 'windows' && activeTab === 'deletedRecovery') ||
      (selectedPlatform === 'linux' && activeLinuxTab === 'deletedRecovery'),
  );

  const analysisMutationPending =
    summaryMutation.isPending || deletedRecovery.scanning || deletedRecovery.reading || deletedRecovery.exporting;

  const labeledExtractionProgress = useMemo(() => labeledProgress(extractionProgress, t), [extractionProgress, t]);
  const linuxNodeCounts = useMemo<Partial<Record<LinuxAnalysisTabKey, number>>>(() => {
    const summary = selectedPlatform === 'linux' ? linuxSummary.data : undefined;
    const counts: Partial<Record<LinuxAnalysisTabKey, number>> = {};
    if (summary) {
      Object.assign(counts, {
        overview: summary.totalCount,
        journal: summary.journalCount + summary.textLogCount,
        login: summary.loginCount,
        commands: summary.bashCommandCount,
        packages: summary.aptEventCount,
        cron: summary.cronJobCount,
        sudo: summary.sudoEventCount,
        systemConfig: summary.systemConfigCount,
        webServices:
          summary.webSiteCount + summary.webAccessLogCount + summary.webErrorLogCount + summary.webFindingCount,
        mysqlServices: summary.mysqlConfigCount + summary.mysqlLogCount + summary.mysqlFindingCount,
        services: summary.systemConfigs.filter(
          (row) => row.configKind === 'systemdServiceUnit' && (row.name ?? row.line).endsWith('.service'),
        ).length,
        docker: summary.systemConfigs.filter(
          (row) => row.configKind === 'dockerContainer' || row.configKind === 'dockerImage',
        ).length,
      });
    }
    if (deletedRecovery.state === 'ready') {
      counts.deletedRecovery = deletedRecovery.total;
    }
    return counts;
  }, [deletedRecovery.state, deletedRecovery.total, linuxSummary.data, selectedPlatform]);

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading;
  const windowsError =
    currentCase.error ??
    systemInfo.error ??
    evidenceSummary.error ??
    registrySummary.error ??
    browserSummary.error ??
    emailSummary.error ??
    eventLogSummary.error ??
    classificationBoard.error ??
    summaryMutation.error ??
    analysisRefreshError;
  const linuxError = currentCase.error ?? linuxSummary.error ?? analysisRefreshError;
  const androidError = currentCase.error ?? androidDeviceInfo.error ?? androidPackages.error ?? analysisRefreshError;

  useLayoutEffect(() => {
    if (sourceEpoch.sync(selectedSourceContextKey)) {
      setAnalysisRefreshError(undefined);
      resetExtractionProgress();
      resetSummaryMutation();
      setExtractionRunning(false);
    }
  }, [resetExtractionProgress, resetSummaryMutation, selectedSourceContextKey, setExtractionRunning, sourceEpoch]);

  useEffect(() => {
    if (!readyDataSources.length) {
      if (selectedDataSourceId) {
        sourceEpoch.sync(undefined);
        resetExtractionProgress();
        resetSummaryMutation();
        setExtractionRunning(false);
        setSelectedDataSourceId(undefined);
      }
      return;
    }

    if (!selectedDataSource) {
      const nextSource = readyDataSources[0];
      const nextContextKey = analysisSourceContextKey(currentCase.data?.id, nextSource);
      sourceEpoch.sync(nextContextKey);
      resetExtractionProgress();
      resetSummaryMutation();
      setExtractionRunning(false);
      setSelectedDataSourceId(nextSource.id);
    }
  }, [
    currentCase.data?.id,
    readyDataSources,
    resetExtractionProgress,
    resetSummaryMutation,
    selectedDataSource,
    selectedDataSourceId,
    sourceEpoch,
    setSelectedDataSourceId,
    setExtractionRunning,
  ]);

  useEffect(
    () => () => {
      sourceEpoch.sync(undefined);
    },
    [sourceEpoch],
  );

  useEffect(() => {
    // Coalesce high-frequency backend progress events before they reach the UI store.
    const pending = new Map<ExtractionCategory, AnalysisExtractionProgress>();
    let flushTimer: ReturnType<typeof setTimeout> | undefined;
    const applyProgress = (category: ExtractionCategory, progress: AnalysisExtractionProgress) => {
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
      setExtractionRunning(
        Object.values(useAnalysisStore.getState().extractionProgress).some((item) => item.status === 'running'),
      );
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
        progress.caseId !== currentCase.data?.id ||
        progress.dataSourceId !== selectedDataSource?.id ||
        !isExtractionCategory(category)
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
  }, [currentCase.data?.id, selectedDataSource?.id, updateExtractionProgress, setExtractionRunning]);

  async function refresh() {
    try {
      if (selectedPlatform === 'android') {
        await Promise.all([androidDeviceInfo.refetch(), androidPackages.refetch()]);
      } else {
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
            pluginModulesQuery.refetch,
          ],
          [linuxSummary.refetch, pluginModulesQuery.refetch],
        );
      }
      setAnalysisRefreshError(undefined);
    } catch (error) {
      setAnalysisRefreshError(error);
    }
  }

  function selectPluginModule(pluginId: string) {
    setActivePluginId(pluginId);
    if (selectedPlatform === 'windows') {
      setActiveTab('plugin');
    } else if (selectedPlatform === 'linux') {
      setActiveLinuxTab('plugin');
    }
  }

  function selectDataSource(id: string) {
    if (id === selectedDataSourceId || analysisMutationPending || sourceEpoch.isBusy) {
      return;
    }
    const nextSource = readyDataSources.find((source) => source.id === id);
    if (!nextSource) {
      return;
    }
    const nextContextKey = analysisSourceContextKey(currentCase.data?.id, nextSource);
    sourceEpoch.sync(nextContextKey);
    resetExtractionProgress();
    resetSummaryMutation();
    setExtractionRunning(false);
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

  return {
    hasCase,
    currentCaseIsSuccess: currentCase.isSuccess,
    loading,
    readyDataSources,
    selectedDataSourceId,
    selectedPlatform,
    analysisMutationPending,
    labeledExtractionProgress,
    linuxNodeCounts,
    activeTab,
    activeLinuxTab,
    activeAndroidTab,
    extractionRunning,
    windowsError,
    linuxError,
    androidError,
    systemInfo,
    evidenceSummary,
    registrySummary,
    registryStructured: registryStructured.data,
    browserSummary,
    emailSummary,
    eventLogSummary,
    eventLogView,
    eventLogLoadContextKey: selectedSourceContextKey ?? '',
    classificationBoard,
    summaryPending: summaryMutation.isPending,
    linuxSummary: linuxSummary.data,
    linuxSummaryLoading: linuxSummary.isLoading,
    linuxSummaryHasMore: Boolean(linuxSummary.hasNextPage),
    linuxSummaryLoadingMore: linuxSummary.isFetchingNextPage,
    linuxSummaryLoadMoreFailed: linuxSummary.isFetchNextPageError,
    linuxSummaryLoadStateKey: linuxSummary.dataUpdatedAt,
    linuxLoadContextKey: selectedSourceContextKey ?? '',
    loadMoreLinuxSummary: () => {
      void linuxSummary.fetchNextPage();
    },
    retryLinuxSummaryLoad: () => linuxSummary.refetch(),
    androidDeviceInfo: androidDeviceInfo.data,
    androidPackages: androidPackages.data,
    androidLoading: androidDeviceInfo.isLoading || androidPackages.isLoading,
    androidPackagesHasMore: Boolean(androidPackages.hasNextPage),
    androidPackagesLoadingMore: androidPackages.isFetchingNextPage,
    loadMoreAndroidPackages: () => {
      void androidPackages.fetchNextPage();
    },
    recoveryModel: deletedRecovery,
    pluginModules: pluginModulesQuery.data ?? [],
    activePluginId,
    selectPluginModule,
    refresh,
    selectDataSource,
    setActiveTab,
    setActiveLinuxTab,
    setActiveAndroidTab,
    setEventLogView,
    downloadSummary,
  };
}

export type AnalysisWorkspaceModel = ReturnType<typeof useAnalysisWorkspaceModel>;
