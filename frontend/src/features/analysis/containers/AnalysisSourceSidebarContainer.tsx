import type { AnalysisSourceSidebarProps } from '../components/analysis-source-sidebar-types';
import { AnalysisSourceSidebarView } from '../components/AnalysisSourceSidebarView';
import { useAnalysisSourceSidebarModel } from '../use-analysis-source-sidebar-model';
export function AnalysisSourceSidebar(props: AnalysisSourceSidebarProps) { return <AnalysisSourceSidebarView model={useAnalysisSourceSidebarModel(props)} />; }
