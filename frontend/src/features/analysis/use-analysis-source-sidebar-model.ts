import { useCallback, useMemo, useState } from 'react';
import { buildAnalysisSourceRows } from './analysis-source-rows';
import type { AnalysisSourceSidebarProps } from './components/analysis-source-sidebar-types';
export function useAnalysisSourceSidebarModel(props: AnalysisSourceSidebarProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const { dataSources, selectedDataSourceId, pluginModules } = props;
  const rows = useMemo(() => buildAnalysisSourceRows(dataSources, selectedDataSourceId, pluginModules, collapsed), [dataSources, selectedDataSourceId, pluginModules, collapsed]);
  const selectOrToggle = useCallback((sourceId: string, selected: boolean, expanded: boolean) => {
    if (!selected) props.onSelectDataSource(sourceId);
    if (selected || !expanded) setCollapsed((current) => { const next = new Set(current); if (next.has(sourceId)) next.delete(sourceId); else next.add(sourceId); return next; });
  }, [props.onSelectDataSource]);
  return { props, rows, selectOrToggle };
}
