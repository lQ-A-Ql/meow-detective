import { Activity } from 'lucide-react';
import { AnalysisExtractionProgress } from '@/features/analysis/components/AnalysisPanels';
import type { AnalysisExtractionProgressInfo } from '@/features/analysis/components/AnalysisPanels';
import type { DataSourceSummary } from '@/types/models';

interface AnalysisProgressDrawerProps {
  source: DataSourceSummary;
  progress: AnalysisExtractionProgressInfo[];
  extractionRunning: boolean;
}

export function AnalysisProgressDrawer({
  source,
  progress,
  extractionRunning,
}: AnalysisProgressDrawerProps) {
  const hasVisibleProgress = extractionRunning || progress.some((item) => item.status !== 'idle');
  if (!hasVisibleProgress) {
    return null;
  }

  return (
    <section data-testid="analysis-progress-drawer" className="min-w-0 max-w-full overflow-hidden border border-forensics-border bg-forensics-panel p-3">
      <div className="mb-3 flex min-w-0 items-center gap-2 text-[11px] text-forensics-text">
        <Activity size={13} className="shrink-0 text-forensics-muted-light" />
        <span className="shrink-0 font-light">数据源提取进度</span>
        <span className="min-w-0 truncate text-forensics-muted" title={source.name}>{source.name}</span>
      </div>
      <div className="space-y-2">
        {progress.map((item) => (
          <AnalysisExtractionProgress key={item.label} progress={item} />
        ))}
      </div>
    </section>
  );
}
