import { Database, FileClock, FileText, Globe, Monitor, Server, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Tabs, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import {
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
  LinuxArtifactsPanel,
  LINUX_ARTIFACT_TAB_KEYS,
  type AnalysisExtractionProgressInfo,
} from '@/features/analysis/components/AnalysisPanels';
import type { LinuxArtifactSummary } from '@/types/models';
import type {
  ExtractionCategory,
  LinuxAnalysisTabKey,
} from '@/features/analysis/types';

const TAB_ICONS: Record<LinuxAnalysisTabKey, typeof Server> = {
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

export interface LinuxAnalysisViewProps {
  activeTab: LinuxAnalysisTabKey;
  onActiveTabChange: (tab: LinuxAnalysisTabKey) => void;
  error?: string;
  onRetry: () => void;
  loading: boolean;
  summary?: LinuxArtifactSummary;
  summaryLoading: boolean;
  progress: Record<ExtractionCategory, AnalysisExtractionProgressInfo>;
}

export function LinuxAnalysisView({
  activeTab,
  onActiveTabChange,
  error,
  onRetry,
  loading,
  summary,
  summaryLoading,
  progress,
}: LinuxAnalysisViewProps) {
  const { t } = useTranslation();

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => onActiveTabChange(value as LinuxAnalysisTabKey)}
      className="h-full min-h-0 flex-1 gap-0"
    >
      <TabsList className="h-auto w-full justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-panel p-0">
        {LINUX_ARTIFACT_TAB_KEYS.map((value) => {
          const Icon = TAB_ICONS[value];
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
        {error ? <AnalysisErrorBanner message={error} onRetry={onRetry} /> : null}
        {loading || summaryLoading ? (
          <AnalysisLoadingPanel
            text={loading ? t('analysis.loading.case') : t('analysis.loading.linuxArtifacts')}
          />
        ) : (
          <LinuxArtifactsPanel
            summary={summary}
            progress={progress.LinuxArtifacts}
            progressByTab={{
              overview: progress.LinuxArtifacts,
              journal: progress.LinuxJournal,
              login: progress.LinuxLogin,
              commands: progress.LinuxCommands,
              packages: progress.LinuxPackages,
              cron: progress.LinuxCron,
              sudo: progress.LinuxSudo,
              systemConfig: progress.LinuxSystemConfig,
              webServices: progress.LinuxWebServices,
              mysqlServices: progress.LinuxMysqlServices,
            }}
            activeTab={activeTab}
          />
        )}
      </div>
    </Tabs>
  );
}
