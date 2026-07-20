import { Activity } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useDataSources } from '@/features/case/hooks';
import { PROGRESS_CATEGORIES_BY_PLATFORM } from '@/features/analysis/types';
import { AnalysisExtractionProgress } from '@/features/analysis/components/AnalysisPanels';
import { labeledProgress, useAnalysisStore } from '@/stores/analysis-store';

export function AnalysisProgressDrawer() {
  const { t } = useTranslation();
  const { data: dataSources } = useDataSources();
  const selectedDataSourceId = useAnalysisStore((state) => state.selectedDataSourceId);
  const extractionProgress = useAnalysisStore((state) => state.extractionProgress);
  const extractionRunning = useAnalysisStore((state) => state.extractionRunning);
  const selectedSource = dataSources?.find((source) => source.id === selectedDataSourceId);
  const progress = labeledProgress(extractionProgress, t);
  const categories = selectedSource
    ? PROGRESS_CATEGORIES_BY_PLATFORM[selectedSource.platform]
    : [];
  const items = categories.map((category) => progress[category]);
  const hasVisibleProgress = extractionRunning || items.some((item) => item.status !== 'idle');

  if (!selectedSource || !hasVisibleProgress) {
    return null;
  }

  return (
    <section data-testid="analysis-progress-drawer" className="border border-forensics-border bg-forensics-panel p-3">
      <div className="mb-3 flex min-w-0 items-center gap-2 text-[11px] text-forensics-text">
        <Activity size={13} className="shrink-0 text-forensics-muted-light" />
        <span className="shrink-0 font-light">数据源提取进度</span>
        <span className="min-w-0 truncate text-forensics-muted" title={selectedSource.name}>{selectedSource.name}</span>
      </div>
      <div className="space-y-2">
        {items.map((item) => (
          <AnalysisExtractionProgress key={item.label} progress={item} />
        ))}
      </div>
    </section>
  );
}
