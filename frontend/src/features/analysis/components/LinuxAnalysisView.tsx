import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
  LinuxArtifactsPanel,
  PluginModulePanel,
} from '@/features/analysis/components/AnalysisPanels';
import type { LinuxArtifactSummary, PluginModule } from '@/types/models';
import type { LinuxAnalysisTabKey } from '@/features/analysis/types';
import { DeletedRecoveryPanel } from '@/features/recovery/components/DeletedRecoveryPanel';
import type { DeletedRecoveryViewModel } from '@/features/recovery/types';

export interface LinuxAnalysisViewProps {
  activeTab: LinuxAnalysisTabKey;
  onActiveTabChange: (tab: LinuxAnalysisTabKey) => void;
  error?: string;
  onRetry: () => void;
  loading: boolean;
  summary?: LinuxArtifactSummary;
  summaryLoading: boolean;
  extractionRunning: boolean;
  recoveryModel: DeletedRecoveryViewModel;
  hasMore?: boolean;
  loadingMore?: boolean;
  loadMoreFailed?: boolean;
  loadContextKey?: string;
  loadStateKey?: string | number;
  onLoadMore?: () => void;
  onRetryLoadMore?: () => unknown;
  pluginModules?: PluginModule[];
  activePluginId?: string;
  dataSourceId?: string;
}

export function LinuxAnalysisView({
  activeTab,
  error,
  onRetry,
  loading,
  summary,
  summaryLoading,
  extractionRunning,
  recoveryModel,
  hasMore,
  loadingMore,
  loadMoreFailed,
  loadContextKey,
  loadStateKey,
  onLoadMore,
  onRetryLoadMore,
  pluginModules,
  activePluginId,
  dataSourceId,
}: LinuxAnalysisViewProps) {
  const { t } = useTranslation();
  const activePluginModule = activeTab === 'plugin'
    ? pluginModules?.find((module) => module.pluginId === activePluginId)
    : undefined;

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col gap-0">
      <ScrollArea className="min-h-0 flex-1" viewportClassName="p-6">
        {error ? <AnalysisErrorBanner message={error} onRetry={onRetry} /> : null}
        {activeTab === 'deletedRecovery' ? (
          <DeletedRecoveryPanel model={recoveryModel} />
        ) : activeTab === 'plugin' ? (
          activePluginModule && dataSourceId ? (
            <PluginModulePanel
              dataSourceId={dataSourceId}
              module={activePluginModule}
              loadContextKey={dataSourceId}
            />
          ) : null
        ) : loading || summaryLoading ? (
          <AnalysisLoadingPanel
            text={loading ? t('analysis.loading.case') : t('analysis.loading.linuxArtifacts')}
          />
        ) : (
          <LinuxArtifactsPanel
            summary={summary}
            activeTab={activeTab}
            extractionRunning={extractionRunning}
            hasMore={hasMore}
            loadingMore={loadingMore}
            loadMoreFailed={loadMoreFailed}
            loadContextKey={loadContextKey}
            loadStateKey={loadStateKey}
            onLoadMore={onLoadMore}
            onRetryLoadMore={onRetryLoadMore}
          />
        )}
      </ScrollArea>
    </div>
  );
}
