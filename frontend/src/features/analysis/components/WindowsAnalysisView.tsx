import { useTranslation } from 'react-i18next';
import { Tabs, TabsContent } from '@/app/components/ui/tabs';
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
import type { AnalysisTabKey } from '@/features/analysis/types';

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
                />
              )}
            </TabsContent>

            <TabsContent value="browser" className="m-0 data-[state=inactive]:hidden">
              {browserSummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.browser')} />
              ) : (
                <BrowserHistoryPanel
                  summary={browserSummary.data}
                />
              )}
            </TabsContent>

            <TabsContent value="email" className="m-0 data-[state=inactive]:hidden">
              {emailSummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.email')} />
              ) : (
                <EmailExtractionPanel summary={emailSummary.data} />
              )}
            </TabsContent>

            <TabsContent value="eventlogs" className="m-0 data-[state=inactive]:hidden">
              {eventLogSummary.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.eventLogs')} />
              ) : (
                <EventLogPanel summary={eventLogSummary.data} />
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
