import { useState } from 'react';
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
  type AnalysisExtractionProgressInfo,
  type AnalysisExtractionProgressState,
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

type AnalysisTabKey = 'system' | 'evidence' | 'registry' | 'browser' | 'email' | 'eventlogs' | 'files' | 'report';

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

type ExtractionCategory = 'Registry' | 'BrowserHistory' | 'Email' | 'EventLogs';

const EXTRACTION_CATEGORIES: ExtractionCategory[] = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
];

function emptyProgress(label: string): AnalysisExtractionProgressInfo {
  return {
    label,
    status: 'idle',
    scannedCount: 0,
    artifactCount: 0,
    timelineEventCount: 0,
    warnings: [],
  };
}

function statusFromRun(status: string): AnalysisExtractionProgressState {
  if (status === 'failed' || status === 'unavailable') {
    return 'failed';
  }
  if (status === 'partial') {
    return 'partial';
  }
  return 'success';
}

function defaultProgressMap(t: (key: string) => string): Record<ExtractionCategory, AnalysisExtractionProgressInfo> {
  return {
    Registry: emptyProgress(t('analysis.extraction.Registry')),
    BrowserHistory: emptyProgress(t('analysis.extraction.BrowserHistory')),
    Email: emptyProgress(t('analysis.extraction.Email')),
    EventLogs: emptyProgress(t('analysis.extraction.EventLogs')),
  };
}

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
  const [extractionProgress, setExtractionProgress] = useState(() => defaultProgressMap(t));
  const [extractionRunning, setExtractionRunning] = useState(false);

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
    setExtractionProgress(defaultProgressMap(t));
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
        setExtractionProgress((current) => ({
          ...current,
          [category]: {
            ...current[category],
            status: 'running',
            warnings: [],
            error: undefined,
          },
        }));

        try {
          const run = await extractionRun.mutateAsync({ categories: [category] });
          setExtractionProgress((current) => ({
            ...current,
            [category]: {
              label: t(`analysis.extraction.${category}`),
              status: statusFromRun(run.status),
              scannedCount: run.scannedCount,
              artifactCount: run.artifactCount,
              timelineEventCount: run.timelineEventCount,
              warnings: run.warnings,
            },
          }));
          await refetchByCategory[category]();
          if (category === 'Registry') {
            await refetchRegistryStructured();
          }
        } catch (err) {
          setExtractionProgress((current) => ({
            ...current,
            [category]: {
              ...current[category],
              status: 'failed',
              error: errorMessage(err),
            },
          }));
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
      progress={extractionProgress[category]}
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
          className="shrink-0 border-b border-forensics-border bg-forensics-panel px-6 py-4"
        >
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
            {extractionProgressCards}
          </div>
        </div>
      ) : null}

      {!hasCase && currentCase.isSuccess ? (
        <AnalysisEmptyState
          demoPending={demoCase.isPending}
          onLoadDemoCase={loadDemoCase}
        />
      ) : (
        <Tabs defaultValue="system" className="min-h-0 flex-1 gap-0">
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
                      progress={extractionProgress.Registry}
                    />
                  )}
                </TabsContent>

                <TabsContent value="browser" className="m-0 data-[state=inactive]:hidden">
                  {browserSummary.isLoading ? (
                    <AnalysisLoadingPanel text={t('analysis.loading.browser')} />
                  ) : (
                    <BrowserHistoryPanel
                      summary={browserSummary.data}
                      progress={extractionProgress.BrowserHistory}
                    />
                  )}
                </TabsContent>

                <TabsContent value="email" className="m-0 data-[state=inactive]:hidden">
                  {emailSummary.isLoading ? (
                    <AnalysisLoadingPanel text={t('analysis.loading.email')} />
                  ) : (
                    <EmailExtractionPanel
                      summary={emailSummary.data}
                      progress={extractionProgress.Email}
                    />
                  )}
                </TabsContent>

                <TabsContent value="eventlogs" className="m-0 data-[state=inactive]:hidden">
                  {eventLogSummary.isLoading ? (
                    <AnalysisLoadingPanel text={t('analysis.loading.eventLogs')} />
                  ) : (
                    <EventLogPanel
                      summary={eventLogSummary.data}
                      progress={extractionProgress.EventLogs}
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
