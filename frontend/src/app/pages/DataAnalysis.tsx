import { useState } from 'react';
import { Database, Download, FileText, Globe, Mail, Monitor, Shield } from 'lucide-react';
import { useCreateAnalysisDemoCase, useCurrentCase } from '@/features/case/hooks';
import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useBrowserHistorySummary,
  useEmailExtractionSummary,
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

const analysisTabs = [
  { value: 'system', label: '系统信息', icon: Monitor },
  { value: 'evidence', label: '证据分类', icon: Shield },
  { value: 'registry', label: '注册表', icon: Database },
  { value: 'browser', label: '浏览器记录', icon: Globe },
  { value: 'email', label: '邮件信息', icon: Mail },
  { value: 'files', label: '文件分类', icon: FileText },
  { value: 'report', label: '报告', icon: Download },
] as const;

type ExtractionCategory = 'Registry' | 'BrowserHistory' | 'Email';

const extractionCategories: Array<{ key: ExtractionCategory; label: string }> = [
  { key: 'Registry', label: '注册表提取' },
  { key: 'BrowserHistory', label: '浏览器记录提取' },
  { key: 'Email', label: '邮件信息提取' },
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

function defaultProgressMap(): Record<ExtractionCategory, AnalysisExtractionProgressInfo> {
  return {
    Registry: emptyProgress('注册表提取'),
    BrowserHistory: emptyProgress('浏览器记录提取'),
    Email: emptyProgress('邮件信息提取'),
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
  const classifications = useAnalysisClassifications(1000);
  const summaryMutation = useGenerateAnalysisSummary();
  const [extractionProgress, setExtractionProgress] = useState(defaultProgressMap);
  const [extractionRunning, setExtractionRunning] = useState(false);

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading || demoCase.isPending;
  const error = currentCase.error
    ?? systemInfo.error
    ?? evidenceSummary.error
    ?? registrySummary.error
    ?? browserSummary.error
    ?? emailSummary.error
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
      browserSummary.refetch(),
      emailSummary.refetch(),
      classifications.refetch(),
    ]);
  }

  async function runEvidenceScan() {
    await evidenceScan.mutateAsync([]);
    await evidenceSummary.refetch();
  }

  async function runExtraction() {
    setExtractionRunning(true);
    setExtractionProgress(defaultProgressMap());
    const refetchByCategory: Record<ExtractionCategory, () => Promise<unknown>> = {
      Registry: registrySummary.refetch,
      BrowserHistory: browserSummary.refetch,
      Email: emailSummary.refetch,
    };

    try {
      for (const category of extractionCategories) {
        setExtractionProgress((current) => ({
          ...current,
          [category.key]: {
            ...current[category.key],
            status: 'running',
            warnings: [],
            error: undefined,
          },
        }));

        try {
          const run = await extractionRun.mutateAsync({ categories: [category.key] });
          setExtractionProgress((current) => ({
            ...current,
            [category.key]: {
              label: category.label,
              status: statusFromRun(run.status),
              scannedCount: run.scannedCount,
              artifactCount: run.artifactCount,
              timelineEventCount: run.timelineEventCount,
              warnings: run.warnings,
            },
          }));
          await refetchByCategory[category.key]();
        } catch (err) {
          setExtractionProgress((current) => ({
            ...current,
            [category.key]: {
              ...current[category.key],
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

  const extractionProgressCards = extractionCategories.map((category) => (
    <AnalysisExtractionProgress
      key={category.key}
      progress={extractionProgress[category.key]}
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
          className="shrink-0 border-b border-[#e8e8e8] bg-[#fcfcfc] px-6 py-4"
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
          <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-[#e0e0e0] bg-[#fafafa] p-0">
            {analysisTabs.map(({ value, label, icon: Icon }) => (
              <TabsTrigger
                key={value}
                value={value}
                className="h-auto flex-none rounded-none border-x-0 border-t-0 border-b-2 border-transparent bg-transparent px-5 py-3 text-[12px] data-[state=active]:border-[#111] data-[state=active]:bg-transparent"
              >
                <Icon size={14} />
                {label}
              </TabsTrigger>
            ))}
          </TabsList>

          <div className="min-h-0 flex-1 overflow-auto p-6">
            {error ? (
              <AnalysisErrorBanner message={errorMessage(error)} onRetry={refresh} />
            ) : null}

            {loading ? (
              <AnalysisLoadingPanel text="正在加载案件..." />
            ) : (
              <>
                <TabsContent value="system" forceMount className="m-0 data-[state=inactive]:hidden">
                  {systemInfo.isLoading ? (
                    <AnalysisLoadingPanel text="正在解析 Registry/EVTX 系统信息..." />
                  ) : (
                    <SystemInfoPanel systemInfo={systemInfo.data} />
                  )}
                </TabsContent>

                <TabsContent value="evidence" forceMount className="m-0 data-[state=inactive]:hidden">
                  {evidenceSummary.isLoading ? (
                    <AnalysisLoadingPanel text="正在发现证据语义类别..." />
                  ) : (
                    <EvidenceClassificationPanel
                      summary={evidenceSummary.data}
                      pending={evidenceScan.isPending}
                      onRun={runEvidenceScan}
                    />
                  )}
                </TabsContent>

                <TabsContent value="registry" forceMount className="m-0 data-[state=inactive]:hidden">
                  {registrySummary.isLoading ? (
                    <AnalysisLoadingPanel text="正在读取注册表提取结果..." />
                  ) : (
                    <RegistryExtractionPanel
                      summary={registrySummary.data}
                      structured={registryStructured.data}
                      progress={extractionProgress.Registry}
                    />
                  )}
                </TabsContent>

                <TabsContent value="browser" forceMount className="m-0 data-[state=inactive]:hidden">
                  {browserSummary.isLoading ? (
                    <AnalysisLoadingPanel text="正在读取浏览器记录..." />
                  ) : (
                    <BrowserHistoryPanel
                      summary={browserSummary.data}
                      progress={extractionProgress.BrowserHistory}
                    />
                  )}
                </TabsContent>

                <TabsContent value="email" forceMount className="m-0 data-[state=inactive]:hidden">
                  {emailSummary.isLoading ? (
                    <AnalysisLoadingPanel text="正在读取邮件信息..." />
                  ) : (
                    <EmailExtractionPanel
                      summary={emailSummary.data}
                      progress={extractionProgress.Email}
                    />
                  )}
                </TabsContent>

                <TabsContent value="files" forceMount className="m-0 data-[state=inactive]:hidden">
                  {classifications.isLoading ? (
                    <AnalysisLoadingPanel text="正在按元数据分类文件..." />
                  ) : (
                    <FileClassificationPanel classifications={classifications.data ?? []} />
                  )}
                </TabsContent>

                <TabsContent value="report" forceMount className="m-0 data-[state=inactive]:hidden">
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
