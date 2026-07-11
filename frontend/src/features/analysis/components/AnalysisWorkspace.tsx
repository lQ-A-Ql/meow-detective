import { useEffect, useLayoutEffect, useMemo, useRef, type ComponentType } from 'react';
import { Monitor, Server } from 'lucide-react';
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
import { AnalysisProgressOverview } from '@/features/analysis/components/panels/SystemInfoPanel';
import { LinuxAnalysisView } from '@/features/analysis/components/LinuxAnalysisView';
import { WindowsAnalysisView } from '@/features/analysis/components/WindowsAnalysisView';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { errorMessage } from '@/lib/errors';
import {
  AnalysisSourceEpoch,
  EXTRACTION_CATEGORIES_BY_PLATFORM,
  LINUX_PROGRESS_CATEGORIES,
  PROGRESS_CATEGORIES_BY_PLATFORM,
  analysisSourceContextKey,
  type AnalysisPlatformView,
  type ExtractionCategory,
  isExtractionCategory,
} from '@/features/analysis/types';
import {
  labeledProgress,
  statusFromRun,
  useAnalysisStore,
} from '@/stores/analysis-store';

const PLATFORM_ICONS: Record<AnalysisPlatformView, ComponentType<{ size?: number | string }>> = {
  windows: Monitor,
  linux: Server,
};

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
  const progressExpanded = useAnalysisStore((s) => s.progressExpanded);
  const activeTab = useAnalysisStore((s) => s.activeTab);
  const activeLinuxTab = useAnalysisStore((s) => s.activeLinuxTab);
  const updateExtractionProgress = useAnalysisStore((s) => s.updateExtractionProgress);
  const resetExtractionProgress = useAnalysisStore((s) => s.resetExtractionProgress);
  const setExtractionRunning = useAnalysisStore((s) => s.setExtractionRunning);
  const setProgressExpanded = useAnalysisStore((s) => s.setProgressExpanded);
  const setActiveTab = useAnalysisStore((s) => s.setActiveTab);
  const setActiveLinuxTab = useAnalysisStore((s) => s.setActiveLinuxTab);

  const analysisMutationPending = evidenceScan.isPending
    || extractionRun.isPending
    || summaryMutation.isPending
    || extractionRunning;

  const progressCategories = selectedPlatform
    ? PROGRESS_CATEGORIES_BY_PLATFORM[selectedPlatform]
    : [];

  const labeledExtractionProgress = useMemo(
    () => labeledProgress(extractionProgress, t),
    [extractionProgress, t],
  );

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
  const PlatformIcon = selectedPlatform ? PLATFORM_ICONS[selectedPlatform] : null;

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
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-white">
      <AnalysisHeader
        loading={loading}
        hasCase={hasCase}
        extractionPending={extractionRun.isPending || extractionRunning}
        dataSourceSwitchDisabled={analysisMutationPending}
        extractionRun={extractionRun.data}
        onRefresh={refresh}
        onRunExtraction={runExtraction}
        dataSources={readyDataSources}
        selectedDataSourceId={selectedDataSourceId}
        onSelectDataSource={selectDataSource}
      />

      {hasCase ? (
        <AnalysisProgressOverview
          progress={progressCategories.map((category) => labeledExtractionProgress[category])}
          expanded={progressExpanded}
          onExpandedChange={setProgressExpanded}
        />
      ) : null}

      {!hasCase && currentCase.isSuccess ? (
        <AnalysisEmptyState />
      ) : (
        <Tabs
          value={selectedPlatform ?? ''}
          className="min-h-0 flex-1 gap-0"
        >
          {selectedPlatform && PlatformIcon ? (
            <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-bg-subtle px-6 py-2">
              <TabsTrigger
                value={selectedPlatform}
                className="h-8 flex-none items-center gap-2 rounded-md border border-transparent px-4 text-[12px] data-[state=active]:border-forensics-border data-[state=active]:bg-white"
              >
                <PlatformIcon size={14} />
                {t(`analysis.platformViews.${selectedPlatform}`)}
              </TabsTrigger>
            </TabsList>
          ) : null}

          <TabsContent value="windows" className="m-0 min-h-0 flex-1 data-[state=inactive]:hidden">
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
              progress={labeledExtractionProgress}
              evidencePending={evidenceScan.isPending}
              onRunEvidence={runEvidenceScan}
              summaryPending={summaryMutation.isPending}
              onDownloadSummary={downloadSummary}
            />
          </TabsContent>

          <TabsContent value="linux" className="m-0 min-h-0 flex-1 data-[state=inactive]:hidden">
            <LinuxAnalysisView
              activeTab={activeLinuxTab}
              onActiveTabChange={setActiveLinuxTab}
              error={linuxError ? errorMessage(linuxError) : undefined}
              onRetry={refresh}
              loading={loading}
              summary={linuxSummary.data}
              summaryLoading={linuxSummary.isLoading}
              progress={labeledExtractionProgress}
            />
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
