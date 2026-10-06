import type { DataSourceSummary,PluginModule } from '@/types/models';
import type { AnalysisExtractionProgressInfo,AndroidAnalysisTabKey,AnalysisTabKey,ExtractionCategory,LinuxAnalysisTabKey } from '../types';
export interface AnalysisSourceSidebarProps {
  dataSources: DataSourceSummary[];
  selectedDataSourceId?: string;
  disabled?: boolean;
  progress: Record<ExtractionCategory, AnalysisExtractionProgressInfo>;
  linuxNodeCounts?: Partial<Record<LinuxAnalysisTabKey, number>>;
  activeWindowsTab: AnalysisTabKey;
  activeLinuxTab: LinuxAnalysisTabKey;
  activeAndroidTab: AndroidAnalysisTabKey;
  /** Plugin modules of the currently selected data source (already fetched). */
  pluginModules?: PluginModule[];
  activePluginId?: string;
  onSelectDataSource: (id: string) => void;
  onWindowsTabChange: (tab: AnalysisTabKey) => void;
  onLinuxTabChange: (tab: LinuxAnalysisTabKey) => void;
  onAndroidTabChange: (tab: AndroidAnalysisTabKey) => void;
  onSelectPluginModule?: (pluginId: string) => void;
}

