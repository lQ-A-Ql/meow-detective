import { useTranslation } from 'react-i18next';
import { Tabs, TabsContent } from '@/app/components/ui/tabs';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
  AnalysisReportPanel,
  BrowserHistoryPanel,
  EmailExtractionPanel,
  EventLogPanel,
  EvidenceClassificationPanel,
  RegistryExtractionPanel,
  PluginModulePanel,
  SystemInfoPanel,
} from '@/features/analysis/components/AnalysisPanels';
import { FileClassificationBoardContainer } from '@/features/analysis/containers/FileClassificationBoardContainer';
import type {
  FileClassificationBoard as FileClassificationBoardData,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  EmailExtractionSummary,
  EvidenceClassificationSummary,
  EvtxEventSummary,
  EvtxEventView,
  RegistryExtractionSummary,
  RegistryStructuredSummary,
  PluginModule,
} from '@/types/models';
import type { AnalysisTabKey } from '@/features/analysis/types';
import { DeletedRecoveryPanel } from '@/features/recovery/components/DeletedRecoveryPanel';
import type { DeletedRecoveryViewModel } from '@/features/recovery/types';

interface QueryState<T> {
  data?: T;
  isLoading: boolean;
}

interface InfiniteQueryState<T> extends QueryState<T> {
  hasNextPage?: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  dataUpdatedAt: number;
  fetchNextPage: () => Promise<unknown>;
  refetch: () => Promise<unknown>;
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
  eventLogSummary: InfiniteQueryState<EvtxEventSummary>;
  eventLogView: EvtxEventView;
  eventLogLoadContextKey: string;
  onEventLogViewChange: (view: EvtxEventView) => void;
  classificationBoard: QueryState<FileClassificationBoardData>;
  evidencePending: boolean;
  onRunEvidence: () => void;
  summaryPending: boolean;
  onDownloadSummary: () => void;
  recoveryModel: DeletedRecoveryViewModel;
  pluginModules?: PluginModule[];
  activePluginId?: string;
  dataSourceId?: string;
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
  eventLogView,
  eventLogLoadContextKey,
  onEventLogViewChange,
  classificationBoard,
  evidencePending,
  onRunEvidence,
  summaryPending,
  onDownloadSummary,
  recoveryModel,
  pluginModules,
  activePluginId,
  dataSourceId,
}: WindowsAnalysisViewProps) {
  const { t } = useTranslation();
  const activePluginModule = activeTab === 'plugin'
    ? pluginModules?.find((module) => module.pluginId === activePluginId)
    : undefined;

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => onActiveTabChange(value as AnalysisTabKey)}
      className="h-full min-h-0 flex-1 gap-0"
    >
      <ScrollArea className="min-h-0 flex-1" viewportClassName="p-6">
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
                <EventLogPanel
                  summary={eventLogSummary.data}
                  activeView={eventLogView}
                  onActiveViewChange={onEventLogViewChange}
                  loadContextKey={eventLogLoadContextKey}
                  loadStateKey={eventLogSummary.dataUpdatedAt}
                  hasMore={Boolean(eventLogSummary.hasNextPage)}
                  loadingMore={eventLogSummary.isFetchingNextPage}
                  loadMoreFailed={eventLogSummary.isFetchNextPageError}
                  onLoadMore={() => {
                    void eventLogSummary.fetchNextPage();
                  }}
                  onRetryLoadMore={() => {
                    return eventLogSummary.refetch();
                  }}
                />
              )}
            </TabsContent>

            <TabsContent value="files" className="m-0 data-[state=inactive]:hidden">
              {classificationBoard.isLoading ? (
                <AnalysisLoadingPanel text={t('analysis.loading.files')} />
              ) : (
                <FileClassificationBoardContainer board={classificationBoard.data} />
              )}
            </TabsContent>

            <TabsContent value="report" className="m-0 data-[state=inactive]:hidden">
              <AnalysisReportPanel pending={summaryPending} onDownload={onDownloadSummary} />
            </TabsContent>

            <TabsContent value="deletedRecovery" className="m-0 data-[state=inactive]:hidden">
              <DeletedRecoveryPanel model={recoveryModel} />
            </TabsContent>

            <TabsContent value="plugin" className="m-0 data-[state=inactive]:hidden">
              {activePluginModule && dataSourceId ? (
                <PluginModulePanel
                  dataSourceId={dataSourceId}
                  module={activePluginModule}
                  loadContextKey={dataSourceId}
                />
              ) : null}
            </TabsContent>
          </>
        )}
      </ScrollArea>
    </Tabs>
  );
}
