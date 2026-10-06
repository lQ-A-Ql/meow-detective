import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { VirtualList } from '@/components/lists/VirtualList';
import { AnalysisSourceRow } from './AnalysisSourceRow';
import { buildAnalysisSourceRows, type AnalysisSourceRowData } from './analysis-source-rows';
import type { AnalysisSourceSidebarProps } from './analysis-source-sidebar-types';
export type { AnalysisSourceSidebarProps } from './analysis-source-sidebar-types';

const getRowKey = (row: AnalysisSourceRowData) => row.key;

export function AnalysisSourceSidebar(props: AnalysisSourceSidebarProps) {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const { dataSources, selectedDataSourceId, pluginModules } = props;
  const rows = useMemo(() => buildAnalysisSourceRows(dataSources, selectedDataSourceId, pluginModules, collapsed),
    [dataSources, selectedDataSourceId, pluginModules, collapsed]);
  const selectOrToggle = useCallback((sourceId: string, selected: boolean, expanded: boolean) => {
    if (!selected) props.onSelectDataSource(sourceId);
    if (selected || !expanded) {
      setCollapsed((current) => {
        const next = new Set(current);
        if (next.has(sourceId)) next.delete(sourceId);
        else next.add(sourceId);
        return next;
      });
    }
  }, [props.onSelectDataSource]);
  return (
    <aside className="flex min-h-0 w-64 shrink-0 flex-col border-r border-forensics-border bg-forensics-panel" aria-label={t('analysis.sidebar.treeLabel')}>
      <div className="border-b border-forensics-border px-3 py-3">
        <div className="text-[13px] font-light text-forensics-text">{t('analysis.sidebar.title')}</div>
        <div className="mt-1 text-[11px] text-forensics-muted">{t('analysis.sidebar.description')}</div>
      </div>
      <VirtualList items={rows} getItemKey={getRowKey} estimateSize={30} className="flex-1"
        viewportClassName="p-2" ariaLabel={t('analysis.sidebar.treeLabel')}
        renderItem={(row) => <AnalysisSourceRow row={row} model={props} onSelectOrToggle={selectOrToggle} />} />
    </aside>
  );
}
