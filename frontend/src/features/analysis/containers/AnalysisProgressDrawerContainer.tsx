import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { AnalysisProgressDrawer } from '@/features/analysis/components/AnalysisProgressDrawer';
import { useDataSources } from '@/features/case/hooks';
import { PROGRESS_CATEGORIES_BY_PLATFORM } from '@/features/analysis/types';
import { labeledProgress, useAnalysisStore } from '@/stores/analysis-store';

export function AnalysisProgressDrawerContainer() {
  const { t } = useTranslation();
  const { data: dataSources } = useDataSources();
  const selectedDataSourceId = useAnalysisStore((state) => state.selectedDataSourceId);
  const extractionProgress = useAnalysisStore((state) => state.extractionProgress);
  const extractionRunning = useAnalysisStore((state) => state.extractionRunning);
  const selectedSource = dataSources?.find((source) => source.id === selectedDataSourceId);
  const progress = useMemo(() => labeledProgress(extractionProgress, t), [extractionProgress, t]);

  if (!selectedSource) {
    return null;
  }

  const items = PROGRESS_CATEGORIES_BY_PLATFORM[selectedSource.platform]
    .map((category) => progress[category]);
  return (
    <AnalysisProgressDrawer
      source={selectedSource}
      progress={items}
      extractionRunning={extractionRunning}
    />
  );
}
