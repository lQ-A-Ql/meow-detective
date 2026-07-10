import { useEffect, useMemo, type ComponentType } from 'react';
import { Database, Download, FileClock, FileText, Globe, Mail, Monitor, Server, Shield } from 'lucide-react';
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
  AnalysisErrorBanner,
  AnalysisExtractionProgress,
  AnalysisHeader,
  AnalysisLoadingPanel,
  AnalysisReportPanel,
  BrowserHistoryPanel,
  EmailExtractionPanel,
  EventLogPanel,
  EvidenceClassificationPanel,
  FileClassificationPanel,
  LinuxArtifactsPanel,
  LINUX_ARTIFACT_TAB_KEYS,
  RegistryExtractionPanel,
  SystemInfoPanel,
} from '@/features/analysis/components/AnalysisPanels';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/app/components/ui/tabs';
import { Button } from '@/app/components/ui/button';
import { inferDataSourcePlatform } from '@/lib/data-source-utils';
import { errorMessage } from '@/lib/errors';
import {
  type AnalysisTabKey,
  type AnalysisPlatformView,
  type ExtractionCategory,
  type LinuxAnalysisTabKey,
  labeledProgress,
  statusFromRun,
  useAnalysisStore,
} from '@/stores/analysis-store';

const ANALYSIS_TAB_KEYS: AnalysisTabKey[] = [
  'system',
  'evidence',
  'registry',
  'browser',
  'email',
  'eventlogs',
  'files',
  'report',
];

const ANALYSIS_PLATFORM_VIEWS: AnalysisPlatformView[] = ['windows', 'linux'];

const PLATFORM_ICONS: Record<AnalysisPlatformView, ComponentType<{ size?: number | string }>> = {
  windows: Monitor,
  linux: Server,
};

const TAB_ICONS: Record<AnalysisTabKey, ComponentType<{ size?: number | string }>> = {
  system: Monitor,
  evidence: Shield,
  registry: Database,
  browser: Globe,
  email: Mail,
  eventlogs: FileClock,
  files: FileText,
  report: Download,
};

const LINUX_TAB_ICONS: Record<LinuxAnalysisTabKey, ComponentType<{ size?: number | string }>> = {
  overview: Server,
  journal: FileClock,
  login: Monitor,
  commands: FileText,
  packages: Database,
  cron: FileClock,
  sudo: Shield,
  systemConfig: Database,
  webServices: Globe,
  mysqlServices: Database,
};

const WINDOWS_EXTRACTION_CATEGORIES: ExtractionCategory[] = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
];

const LINUX_EXTRACTION_CATEGORIES: ExtractionCategory[] = ['LinuxArtifacts'];

const LINUX_PROGRESS_CATEGORIES: ExtractionCategory[] = [
  'LinuxJournal',
  'LinuxLogin',
  'LinuxCommands',
  'LinuxPackages',
  'LinuxCron',
  'LinuxSudo',
  'LinuxSystemConfig',
  'LinuxWebServices',
  'LinuxMysqlServices',
];

const EXTRACTION_CATEGORIES_BY_VIEW: Record<AnalysisPlatformView, ExtractionCategory[]> = {
  windows: WINDOWS_EXTRACTION_CATEGORIES,
  linux: LINUX_EXTRACTION_CATEGORIES,
};

const PROGRESS_CATEGORIES_BY_VIEW: Record<AnalysisPlatformView, ExtractionCategory[]> = {
  windows: WINDOWS_EXTRACTION_CATEGORIES,
  linux: LINUX_PROGRESS_CATEGORIES,
};

export function AnalysisWorkspace() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const selectedDataSourceId = useAnalysisStore((s) => s.selectedDataSourceId);
  const setSelectedDataSourceId = useAnalysisStore((s) => s.setSelectedDataSourceId);

  const systemInfo = useAnalysisSystemInfo(selectedDataSourceId);
  const evidenceSummary = useEvidenceClassificationSummary(selectedDataSourceId);
  const evidenceScan = useRunEvidenceClassification();
  const extractionRun = useRunAnalysisExtraction();
  const registrySummary = useRegistryExtractionSummary({ dataSourceId: selectedDataSourceId, limit: 200 });
  const registryStructured = useRegistryStructuredSummary(selectedDataSourceId);
  const browserSummary = useBrowserHistorySummary({ dataSourceId: selectedDataSourceId, limit: 200 });
  const emailSummary = useEmailExtractionSummary({ dataSourceId: selectedDataSourceId, limit: 200 });
  const eventLogSummary = useEvtxEventSummary({ dataSourceId: selectedDataSourceId, limit: 200 });
  const linuxSummary = useLinuxArtifactSummary({ dataSourceId: selectedDataSourceId, limit: 200 });
  const classifications = useAnalysisClassifications(selectedDataSourceId, 1000);
  const summaryMutation = useGenerateAnalysisSummary(selectedDataSourceId);

  const extractionProgress = useAnalysisStore((s) => s.extractionProgress);
  const extractionRunning = useAnalysisStore((s) => s.extractionRunning);
  const progressExpanded = useAnalysisStore((s) => s.progressExpanded);
  const activePlatformView = useAnalysisStore((s) => s.activePlatformView);
  const activeTab = useAnalysisStore((s) => s.activeTab);
  const activeLinuxTab = useAnalysisStore((s) => s.activeLinuxTab);
  const updateExtractionProgress = useAnalysisStore((s) => s.updateExtractionProgress);
  const resetExtractionProgress = useAnalysisStore((s) => s.resetExtractionProgress);
  const setExtractionRunning = useAnalysisStore((s) => s.setExtractionRunning);
  const setProgressExpanded = useAnalysisStore((s) => s.setProgressExpanded);
  const setActivePlatformView = useAnalysisStore((s) => s.setActivePlatformView);
  const setActiveTab = useAnalysisStore((s) => s.setActiveTab);
  const setActiveLinuxTab = useAnalysisStore((s) => s.setActiveLinuxTab);
  const selectedDataSource = useMemo(
    () => dataSources?.find((source) => source.id === selectedDataSourceId),
    [dataSources, selectedDataSourceId],
  );

  const extractionCategories = EXTRACTION_CATEGORIES_BY_VIEW[activePlatformView];
  const progressCategories = PROGRESS_CATEGORIES_BY_VIEW[activePlatformView];

  const labeledExtractionProgress = useMemo(
    () => labeledProgress(extractionProgress, t),
    [extractionProgress, t],
  );

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading;
  const sharedError = currentCase.error
    ?? evidenceScan.error
    ?? extractionRun.error;
  const windowsError = sharedError
    ?? systemInfo.error
    ?? evidenceSummary.error
    ?? registrySummary.error
    ?? browserSummary.error
    ?? emailSummary.error
    ?? eventLogSummary.error
    ?? classifications.error
    ?? summaryMutation.error;
  const linuxError = sharedError
    ?? linuxSummary.error;

  useEffect(() => {
    if (!selectedDataSourceId && dataSources?.length) {
      setSelectedDataSourceId(dataSources[0].id);
      return;
    }

    if (!selectedDataSource) {
      return;
    }

    const platform = inferDataSourcePlatform(selectedDataSource);
    if (platform === 'windows' || platform === 'linux') {
      setActivePlatformView(platform);
    }
  }, [dataSources, selectedDataSource, selectedDataSourceId, setActivePlatformView, setSelectedDataSourceId]);

  async function refresh() {
    await Promise.all([
      systemInfo.refetch(),
      evidenceSummary.refetch(),
      registrySummary.refetch(),
      registryStructured.refetch(),
      browserSummary.refetch(),
      emailSummary.refetch(),
      eventLogSummary.refetch(),
      linuxSummary.refetch(),
      classifications.refetch(),
    ]);
  }

  async function runEvidenceScan() {
    if (!selectedDataSourceId) {
      return;
    }
    await evidenceScan.mutateAsync({ dataSourceId: selectedDataSourceId, categories: [] });
    await evidenceSummary.refetch();
  }

  async function runExtraction() {
    if (!selectedDataSourceId) {
      return;
    }
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
      for (const category of extractionCategories) {
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
            dataSourceId: selectedDataSourceId,
            categories: [category],
          });
          updateExtractionProgress(category, {
            status: statusFromRun(run.status),
            scannedCount: run.scannedCount,
            artifactCount: run.artifactCount,
            timelineEventCount: run.timelineEventCount,
            warnings: run.warnings,
          });
          for (const section of run.sections ?? []) {
            if (!LINUX_PROGRESS_CATEGORIES.includes(section.key as ExtractionCategory)) {
              continue;
            }
            updateExtractionProgress(section.key as ExtractionCategory, {
              status: statusFromRun(section.status),
              scannedCount: section.scannedCount,
              artifactCount: section.artifactCount,
              timelineEventCount: section.timelineEventCount,
              warnings: section.warnings,
              error: undefined,
            });
          }
          await refetchByCategory[category]();
          if (category === 'Registry') {
            await refetchRegistryStructured();
          }
        } catch (err) {
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

      if (activePlatformView === 'windows') {
        await evidenceSummary.refetch();
      }
    } finally {
      setExtractionRunning(false);
    }
  }

  function selectDataSource(id?: string) {
    setSelectedDataSourceId(id);
    const dataSource = dataSources?.find((source) => source.id === id);
    if (!dataSource) {
      return;
    }

    const platform = inferDataSourcePlatform(dataSource);
    if (platform === 'windows' || platform === 'linux') {
      setActivePlatformView(platform);
    }
  }

  async function downloadSummary() {
    if (!selectedDataSourceId) {
      return;
    }
    const summary = await summaryMutation.mutateAsync();
    const blob = new Blob([summary], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'analysis-report.md';
    link.click();
    URL.revokeObjectURL(url);
  }

  const extractionProgressCards = progressCategories.map((category) => (
    <AnalysisExtractionProgress
      key={category}
      progress={labeledExtractionProgress[category]}
    />
  ));

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-white">
      <AnalysisHeader
        loading={loading}
        hasCase={hasCase}
        extractionPending={extractionRun.isPending || extractionRunning}
        extractionRun={extractionRun.data}
        onRefresh={refresh}
        onRunExtraction={runExtraction}
        dataSources={dataSources}
        selectedDataSourceId={selectedDataSourceId}
        onSelectDataSource={selectDataSource}
      />

      {hasCase ? (
        <div
          data-testid="analysis-progress-overview"
          className="shrink-0 border-b border-forensics-border bg-forensics-panel"
        >
          <div className="flex items-center justify-between border-b border-forensics-border px-6 py-2">
            <span className="text-xs font-medium text-forensics-text-secondary">
              {t('analysis.progressDrawer.title')}
            </span>
            <Button
              type="button"
              variant="forensicsSurface"
              size="compact"
              onClick={() => setProgressExpanded(!progressExpanded)}
            >
              <span>{progressExpanded ? t('analysis.progressDrawer.collapse') : t('analysis.progressDrawer.expand')}</span>
            </Button>
          </div>
          {progressExpanded ? (
            <div className="px-6 py-4">
              <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
                {extractionProgressCards}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {!hasCase && currentCase.isSuccess ? (
        <AnalysisEmptyState />
      ) : (
        <Tabs
          value={activePlatformView}
          onValueChange={(value) => setActivePlatformView(value as AnalysisPlatformView)}
          className="min-h-0 flex-1 gap-0"
        >
          <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-bg-subtle px-6 py-2">
            {ANALYSIS_PLATFORM_VIEWS.map((value) => {
              const Icon = PLATFORM_ICONS[value];
              return (
                <TabsTrigger
                  key={value}
                  value={value}
                  className="h-8 flex-none items-center gap-2 rounded-md border border-transparent px-4 text-[12px] data-[state=active]:border-forensics-border data-[state=active]:bg-white"
                >
                  <Icon size={14} />
                  {t(`analysis.platformViews.${value}`)}
                </TabsTrigger>
              );
            })}
          </TabsList>

          <TabsContent value="windows" className="m-0 min-h-0 flex-1 data-[state=inactive]:hidden">
            <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as AnalysisTabKey)} className="h-full min-h-0 flex-1 gap-0">
              <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-panel p-0">
                {ANALYSIS_TAB_KEYS.map((value) => {
                  const Icon = TAB_ICONS[value];
                  return (
                    <TabsTrigger
                      key={value}
                      value={value}
                      className="h-auto flex-none items-center gap-2 whitespace-nowrap rounded-none border-x-0 border-t-0 border-b-2 border-transparent bg-transparent px-5 py-3 text-[12px] data-[state=active]:border-forensics-text data-[state=active]:bg-transparent"
                    >
                      <Icon size={14} />
                      {t(`analysis.tabs.${value}`)}
                    </TabsTrigger>
                  );
                })}
              </TabsList>

              <div className="min-h-0 flex-1 overflow-auto p-6">
                {windowsError ? (
                  <AnalysisErrorBanner message={errorMessage(windowsError)} onRetry={refresh} />
                ) : null}

                {loading ? (
                  <AnalysisLoadingPanel text={t('analysis.loading.case')} />
                ) : (
                  <>
                    <TabsContent value="system" className="m-0 data-[state=inactive]:hidden">
                      {systemInfo.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.systemInfo')} />
                      ) : (
                        <SystemInfoPanel systemInfo={systemInfo.data} />
                      )}
                    </TabsContent>

                    <TabsContent value="evidence" className="m-0 data-[state=inactive]:hidden">
                      {evidenceSummary.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.evidence')} />
                      ) : (
                        <EvidenceClassificationPanel
                          summary={evidenceSummary.data}
                          pending={evidenceScan.isPending}
                          onRun={runEvidenceScan}
                        />
                      )}
                    </TabsContent>

                    <TabsContent value="registry" className="m-0 data-[state=inactive]:hidden">
                      {registrySummary.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.registry')} />
                      ) : (
                        <RegistryExtractionPanel
                          summary={registrySummary.data}
                          structured={registryStructured.data}
                          progress={labeledExtractionProgress.Registry}
                        />
                      )}
                    </TabsContent>

                    <TabsContent value="browser" className="m-0 data-[state=inactive]:hidden">
                      {browserSummary.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.browser')} />
                      ) : (
                        <BrowserHistoryPanel
                          summary={browserSummary.data}
                          progress={labeledExtractionProgress.BrowserHistory}
                        />
                      )}
                    </TabsContent>

                    <TabsContent value="email" className="m-0 data-[state=inactive]:hidden">
                      {emailSummary.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.email')} />
                      ) : (
                        <EmailExtractionPanel
                          summary={emailSummary.data}
                          progress={labeledExtractionProgress.Email}
                        />
                      )}
                    </TabsContent>

                    <TabsContent value="eventlogs" className="m-0 data-[state=inactive]:hidden">
                      {eventLogSummary.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.eventLogs')} />
                      ) : (
                        <EventLogPanel
                          summary={eventLogSummary.data}
                          progress={labeledExtractionProgress.EventLogs}
                        />
                      )}
                    </TabsContent>

                    <TabsContent value="files" className="m-0 data-[state=inactive]:hidden">
                      {classifications.isLoading ? (
                        <AnalysisLoadingPanel text={t('analysis.loading.files')} />
                      ) : (
                        <FileClassificationPanel classifications={classifications.data ?? []} />
                      )}
                    </TabsContent>

                    <TabsContent value="report" className="m-0 data-[state=inactive]:hidden">
                      <AnalysisReportPanel
                        pending={summaryMutation.isPending}
                        onDownload={downloadSummary}
                      />
                    </TabsContent>
                  </>
                )}
              </div>
            </Tabs>
          </TabsContent>

          <TabsContent value="linux" className="m-0 min-h-0 flex-1 data-[state=inactive]:hidden">
            <Tabs
              value={activeLinuxTab}
              onValueChange={(value) => setActiveLinuxTab(value as LinuxAnalysisTabKey)}
              className="h-full min-h-0 flex-1 gap-0"
            >
              <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-panel p-0">
                {LINUX_ARTIFACT_TAB_KEYS.map((value) => {
                  const Icon = LINUX_TAB_ICONS[value];
                  return (
                    <TabsTrigger
                      key={value}
                      value={value}
                      className="h-auto flex-none items-center gap-2 whitespace-nowrap rounded-none border-x-0 border-t-0 border-b-2 border-transparent bg-transparent px-5 py-3 text-[12px] data-[state=active]:border-forensics-text data-[state=active]:bg-transparent"
                    >
                      <Icon size={14} />
                      {t(`linuxArtifacts.tabs.${value}`)}
                    </TabsTrigger>
                  );
                })}
              </TabsList>

              <div className="min-h-0 flex-1 overflow-auto p-6">
                {linuxError ? (
                  <AnalysisErrorBanner message={errorMessage(linuxError)} onRetry={refresh} />
                ) : null}

                {loading || linuxSummary.isLoading ? (
                  <AnalysisLoadingPanel text={loading ? t('analysis.loading.case') : t('analysis.loading.linuxArtifacts')} />
                ) : (
                  <LinuxArtifactsPanel
                    summary={linuxSummary.data}
                    progress={labeledExtractionProgress.LinuxArtifacts}
                    progressByTab={{
                      overview: labeledExtractionProgress.LinuxArtifacts,
                      journal: labeledExtractionProgress.LinuxJournal,
                      login: labeledExtractionProgress.LinuxLogin,
                      commands: labeledExtractionProgress.LinuxCommands,
                      packages: labeledExtractionProgress.LinuxPackages,
                      cron: labeledExtractionProgress.LinuxCron,
                      sudo: labeledExtractionProgress.LinuxSudo,
                      systemConfig: labeledExtractionProgress.LinuxSystemConfig,
                      webServices: labeledExtractionProgress.LinuxWebServices,
                      mysqlServices: labeledExtractionProgress.LinuxMysqlServices,
                    }}
                    activeTab={activeLinuxTab}
                  />
                )}
              </div>
            </Tabs>
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
