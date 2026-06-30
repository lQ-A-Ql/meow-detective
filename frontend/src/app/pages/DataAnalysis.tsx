import { useMemo } from 'react';
import { Database, Download, FileClock, FileText, Globe, Mail, Monitor, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useCreateAnalysisDemoCase, useCurrentCase } from '@/features/case/hooks';
import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useBrowserHistorySummary,
  useEmailExtractionSummary,
  useEvtxEventSummary,
  useEvidenceClassificationSummary,
  useGenerateAnalysisSummary,
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
  RegistryExtractionPanel,
  SystemInfoPanel,
} from '@/components/analysis/AnalysisPanels';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/app/components/ui/tabs';
import { isApiErrorDto } from '@/lib/api/client';
import {
  type AnalysisTabKey,
  type ExtractionCategory,
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

const TAB_ICONS: Record<AnalysisTabKey, React.ComponentType<{ size?: number | string }>> = {
  system: Monitor,
  evidence: Shield,
  registry: Database,
  browser: Globe,
  email: Mail,
  eventlogs: FileClock,
  files: FileText,
  report: Download,
};

const EXTRACTION_CATEGORIES: ExtractionCategory[] = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
];

function errorMessage(error: unknown) {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function DataAnalysis() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const demoCase = useCreateAnalysisDemoCase();
  const systemInfo = useAnalysisSystemInfo();
  const evidenceSummary = useEvidenceClassificationSummary();
  const evidenceScan = useRunEvidenceClassification();
  const extractionRun = useRunAnalysisExtraction();
  const registrySummary = useRegistryExtractionSummary({ limit: 200 });
  const registryStructured = useRegistryStructuredSummary();
  const browserSummary = useBrowserHistorySummary({ limit: 200 });
  const emailSummary = useEmailExtractionSummary({ limit: 200 });
  const eventLogSummary = useEvtxEventSummary({ limit: 200 });
  const classifications = useAnalysisClassifications(1000);
  const summaryMutation = useGenerateAnalysisSummary();

  const extractionProgress = useAnalysisStore((s) => s.extractionProgress);
  const extractionRunning = useAnalysisStore((s) => s.extractionRunning);
  const progressExpanded = useAnalysisStore((s) => s.progressExpanded);
  const activeTab = useAnalysisStore((s) => s.activeTab);
  const updateExtractionProgress = useAnalysisStore((s) => s.updateExtractionProgress);
  const setExtractionRunning = useAnalysisStore((s) => s.setExtractionRunning);
  const setProgressExpanded = useAnalysisStore((s) => s.setProgressExpanded);
  const setActiveTab = useAnalysisStore((s) => s.setActiveTab);

  const labeledExtractionProgress = useMemo(
    () => labeledProgress(extractionProgress, t),
    [extractionProgress, t],
  );

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading || demoCase.isPending;
  const error = currentCase.error
    ?? systemInfo.error
    ?? evidenceSummary.error
    ?? registrySummary.error
    ?? browserSummary.error
    ?? emailSummary.error
    ?? eventLogSummary.error
    ?? classifications.error
    ?? summaryMutation.error
    ?? evidenceScan.error
    ?? extractionRun.error
    ?? demoCase.error;

  async function refresh() {
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
  }

  async function runEvidenceScan() {
    await evidenceScan.mutateAsync([]);
    await evidenceSummary.refetch();
  }

  async function runExtraction() {
    setExtractionRunning(true);
    useAnalysisStore.getState().resetExtractionProgress();
    const refetchByCategory: Record<ExtractionCategory, () => Promise<unknown>> = {
      Registry: registrySummary.refetch,
      BrowserHistory: browserSummary.refetch,
      Email: emailSummary.refetch,
      EventLogs: eventLogSummary.refetch,
    };

    const refetchRegistryStructured = async () => {
      await registryStructured.refetch();
    };

    try {
      for (const category of EXTRACTION_CATEGORIES) {
        updateExtractionProgress(category, {
          status: 'running',
          warnings: [],
          error: undefined,
        });

        try {
          const run = await extractionRun.mutateAsync({ categories: [category] });
          updateExtractionProgress(category, {
            status: statusFromRun(run.status),
            scannedCount: run.scannedCount,
            artifactCount: run.artifactCount,
            timelineEventCount: run.timelineEventCount,
            warnings: run.warnings,
          });
          await refetchByCategory[category]();
          if (category === 'Registry') {
            await refetchRegistryStructured();
          }
        } catch (err) {
          updateExtractionProgress(category, {
            status: 'failed',
            error: errorMessage(err),
          });
        }
      }

      await evidenceSummary.refetch();
    } finally {
      setExtractionRunning(false);
    }
  }

  async function downloadSummary() {
    const summary = await summaryMutation.mutateAsync();
    const blob = new Blob([summary], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'analysis-report.md';
    link.click();
    URL.revokeObjectURL(url);
  }

  async function loadDemoCase() {
    await demoCase.mutateAsync();
  }

  const extractionProgressCards = EXTRACTION_CATEGORIES.map((category) => (
    <AnalysisExtractionProgress
      key={category}
      progress={labeledExtractionProgress[category]}
    />
  ));

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-auto bg-white">
      <AnalysisHeader
        loading={loading}
        hasCase={hasCase}
        demoPending={demoCase.isPending}
        extractionPending={extractionRun.isPending || extractionRunning}
        extractionRun={extractionRun.data}
        onLoadDemoCase={loadDemoCase}
        onRefresh={refresh}
        onRunExtraction={runExtraction}
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
            <button
              type="button"
              onClick={() => setProgressExpanded(!progressExpanded)}
              className="flex items-center gap-1 rounded border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[11px] text-forensics-text hover:bg-forensics-hover"
            >
              <span>{progressExpanded ? t('analysis.progressDrawer.collapse') : t('analysis.progressDrawer.expand')}</span>
            </button>
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
        <AnalysisEmptyState
          demoPending={demoCase.isPending}
          onLoadDemoCase={loadDemoCase}
        />
      ) : (
        <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as AnalysisTabKey)} className="min-h-0 flex-1 gap-0">
          <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-panel p-0">
            {ANALYSIS_TAB_KEYS.map((value) => {
              const Icon = TAB_ICONS[value];
              return (
                <TabsTrigger
                  key={value}
                  value={value}
                  className="h-auto flex-none rounded-none border-x-0 border-t-0 border-b-2 border-transparent bg-transparent px-5 py-3 text-[12px] data-[state=active]:border-forensics-text data-[state=active]:bg-transparent"
                >
                  <Icon size={14} />
                  {t(`analysis.tabs.${value}`)}
                </TabsTrigger>
              );
            })}
          </TabsList>

          <div className="min-h-0 flex-1 overflow-auto p-6">
            {error ? (
              <AnalysisErrorBanner message={errorMessage(error)} onRetry={refresh} />
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
      )}
    </div>
  );
}
