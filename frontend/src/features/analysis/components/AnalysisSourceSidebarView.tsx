import { useTranslation } from 'react-i18next';
import { VirtualList } from '@/components/lists/VirtualList';
import { AnalysisSourceRow } from './AnalysisSourceRow';
import type { AnalysisSourceRowData } from '../analysis-source-rows';
import type { useAnalysisSourceSidebarModel } from '../use-analysis-source-sidebar-model';
const getRowKey = (row: AnalysisSourceRowData) => row.key;
export function AnalysisSourceSidebarView({ model }: { model: ReturnType<typeof useAnalysisSourceSidebarModel> }) {
  const { t } = useTranslation();
  return <aside className="flex min-h-0 w-64 shrink-0 flex-col border-r border-forensics-divider bg-forensics-panel" aria-label={t('analysis.sidebar.treeLabel')}>
    <VirtualList items={model.rows} getItemKey={getRowKey} estimateSize={26} className="flex-1" viewportClassName="p-1" ariaLabel={t('analysis.sidebar.treeLabel')} renderItem={(row) => <AnalysisSourceRow row={row} model={model.props} onSelectOrToggle={model.selectOrToggle} />} />
  </aside>;
}
