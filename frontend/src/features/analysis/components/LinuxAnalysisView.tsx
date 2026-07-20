import { useTranslation } from 'react-i18next';
import { Tabs } from '@/app/components/ui/tabs';
import {
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
  LinuxArtifactsPanel,
} from '@/features/analysis/components/AnalysisPanels';
import type { LinuxArtifactSummary } from '@/types/models';
import type { LinuxAnalysisTabKey } from '@/features/analysis/types';

export interface LinuxAnalysisViewProps {
  activeTab: LinuxAnalysisTabKey;
  onActiveTabChange: (tab: LinuxAnalysisTabKey) => void;
  error?: string;
  onRetry: () => void;
  loading: boolean;
  summary?: LinuxArtifactSummary;
  summaryLoading: boolean;
}

export function LinuxAnalysisView({
  activeTab,
  onActiveTabChange,
  error,
  onRetry,
  loading,
  summary,
  summaryLoading,
}: LinuxAnalysisViewProps) {
  const { t } = useTranslation();

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => onActiveTabChange(value as LinuxAnalysisTabKey)}
      className="h-full min-h-0 flex-1 gap-0"
    >
      <div className="min-h-0 flex-1 overflow-auto p-6">
        {error ? <AnalysisErrorBanner message={error} onRetry={onRetry} /> : null}
        {loading || summaryLoading ? (
          <AnalysisLoadingPanel
            text={loading ? t('analysis.loading.case') : t('analysis.loading.linuxArtifacts')}
          />
        ) : (
          <LinuxArtifactsPanel
            summary={summary}
            activeTab={activeTab}
          />
        )}
      </div>
    </Tabs>
  );
}
