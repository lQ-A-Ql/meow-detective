import {
  Database,
  Download,
  FileClock,
  FileText,
  Globe,
  Mail,
  Monitor,
  Shield,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import {
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
  AnalysisReportPanel,
  BrowserHistoryPanel,
  EmailExtractionPanel,
  EventLogPanel,
  EvidenceClassificationPanel,
  FileClassificationPanel,
  RegistryExtractionPanel,
  SystemInfoPanel,
  type AnalysisExtractionProgressInfo,
} from '@/features/analysis/components/AnalysisPanels';
import type {
  AnalysisFileClassification,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  EmailExtractionSummary,
  EvidenceClassificationSummary,
  EvtxEventSummary,
  RegistryExtractionSummary,
  RegistryStructuredSummary,
} from '@/types/models';
import type { AnalysisTabKey, ExtractionCategory } from '@/features/analysis/types';

const TAB_KEYS: AnalysisTabKey[] = [
  'system',
  'evidence',
  'registry',
  'browser',
  'email',
  'eventlogs',
  'files',
  'report',
];

const TAB_ICONS: Record<AnalysisTabKey, typeof Monitor> = {
  system: Monitor,
  evidence: Shield,
  registry: Database,
  browser: Globe,
  email: Mail,
  eventlogs: FileClock,
  files: FileText,
  report: Download,
};

interface QueryState<T> {
  data?: T;
  isLoading: boolean;
}

export interface WindowsAnalysisViewProps {
  activeTab: AnalysisTabKey;
  onActiveTabChange: (tab: AnalysisTabKey) => void;
  error?: string;
  onRetry: () => void;
  loading: boolean;
  systemInfo: QueryState<AnalysisSystemInfo>;
  evidenceSummary: QueryState<EvidenceClassificationSummary>;
  registrySummary: QueryState<RegistryExtractionSummary>;
  registryStructured?: RegistryStructuredSummary;
  browserSummary: QueryState<BrowserHistorySummary>;
  emailSummary: QueryState<EmailExtractionSummary>;
  eventLogSummary: QueryState<EvtxEventSummary>;
  classifications: QueryState<AnalysisFileClassification[]>;
  progress: Record<ExtractionCategory, AnalysisExtractionProgressInfo>;
  evidencePending: boolean;
  onRunEvidence: () => void;
  summaryPending: boolean;
  onDownloadSummary: () => void;
}

export function WindowsAnalysisView({
  activeTab,
  onActiveTabChange,
  error,
  onRetry,
  loading,
  systemInfo,
  evidenceSummary,
  registrySummary,
  registryStructured,
  browserSummary,
  emailSummary,
  eventLogSummary,
  classifications,
  progress,
  evidencePending,
  onRunEvidence,
  summaryPending,
  onDownloadSummary,
}: WindowsAnalysisViewProps) {
  const { t } = useTranslation();

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => onActiveTabChange(value as AnalysisTabKey)}
      className="h-full min-h-0 flex-1 gap-0"
    >
      <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-panel p-0">
        {TAB_KEYS.map((value) => {
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
        {error ? <AnalysisErrorBanner message={error} onRetry={onRetry} /> : null}
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
                  pending={evidencePending}
                  onRun={onRunEvidence}
                />
              )}
            </TabsContent>

            <TabsContent value="registry" className="m-0 data-[state=inactive]:hidden">
              {registrySummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.registry')} />
              ) : (
                <RegistryExtractionPanel
                  summary={registrySummary.data}
                  structured={registryStructured}
                  progress={progress.Registry}
                />
              )}
            </TabsContent>

            <TabsContent value="browser" className="m-0 data-[state=inactive]:hidden">
              {browserSummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.browser')} />
              ) : (
                <BrowserHistoryPanel
                  summary={browserSummary.data}
                  progress={progress.BrowserHistory}
                />
              )}
            </TabsContent>

            <TabsContent value="email" className="m-0 data-[state=inactive]:hidden">
              {emailSummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.email')} />
              ) : (
                <EmailExtractionPanel summary={emailSummary.data} progress={progress.Email} />
              )}
            </TabsContent>

            <TabsContent value="eventlogs" className="m-0 data-[state=inactive]:hidden">
              {eventLogSummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.eventLogs')} />
              ) : (
                <EventLogPanel summary={eventLogSummary.data} progress={progress.EventLogs} />
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
              <AnalysisReportPanel pending={summaryPending} onDownload={onDownloadSummary} />
            </TabsContent>
          </>
        )}
      </div>
    </Tabs>
  );
}
