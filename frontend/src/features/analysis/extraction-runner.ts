import type { AnalysisExtractionProgressInfo } from '@/features/analysis/components/AnalysisPanels';
import {
  EXTRACTION_CATEGORIES_BY_PLATFORM,
  LINUX_PROGRESS_CATEGORIES,
  isExtractionCategory,
  type AnalysisOperationToken,
  type AnalysisSourceEpoch,
  type ExtractionCategory,
} from '@/features/analysis/types';
import { errorMessage } from '@/lib/errors';
import { statusFromRun } from '@/stores/analysis-store';
import type {
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  DataSourceSummary,
} from '@/types/models';

type ProgressPatch = Partial<Omit<AnalysisExtractionProgressInfo, 'label'>>;
type ProgressUpdater = (category: ExtractionCategory, patch: ProgressPatch) => void;

interface RunSelectedSourceExtractionOptions {
  source?: DataSourceSummary;
  sourceContextKey?: string;
  sourceEpoch: AnalysisSourceEpoch;
  execute: (request: AnalysisExtractionRequest) => Promise<AnalysisExtractionRun>;
  updateProgress: ProgressUpdater;
  resetProgress: () => void;
  setExtractionRunning: (running: boolean) => void;
  setDrawerOpen: (open: boolean) => void;
  setRefreshError: (error: unknown) => void;
  setActiveOperation: (operation?: AnalysisOperationToken) => void;
  isActiveOperation: (operation: AnalysisOperationToken) => boolean;
}

export async function runSelectedSourceExtraction(
  options: RunSelectedSourceExtractionOptions,
) {
  const { source, sourceEpoch, sourceContextKey } = options;
  if (!source) return;
  const operation = sourceEpoch.begin(sourceContextKey);
  if (!operation) return;

  options.setRefreshError(undefined);
  options.setActiveOperation(operation);
  options.setExtractionRunning(true);
  options.setDrawerOpen(true);
  options.resetProgress();
  try {
    for (const category of EXTRACTION_CATEGORIES_BY_PLATFORM[source.platform]) {
      if (!sourceEpoch.isCurrent(operation)) return;
      markRunning(category, options.updateProgress);
      try {
        const run = await options.execute({ dataSourceId: source.id, categories: [category] });
        if (!sourceEpoch.isCurrent(operation)) return;
        applyRun(category, run, options.updateProgress);
      } catch (error) {
        if (!sourceEpoch.isCurrent(operation)) return;
        markFailed(category, error, options.updateProgress);
        options.setRefreshError(error);
      }
    }
  } finally {
    sourceEpoch.finish(operation);
    if (options.isActiveOperation(operation)) {
      options.setActiveOperation(undefined);
      options.setExtractionRunning(false);
    }
  }
}

function markRunning(category: ExtractionCategory, update: ProgressUpdater) {
  const categories = category === 'LinuxArtifacts' ? LINUX_PROGRESS_CATEGORIES : [category];
  for (const progressCategory of categories) {
    update(progressCategory, { status: 'running', warnings: [], error: undefined });
  }
  update(category, { status: 'running', warnings: [], error: undefined });
}

function applyRun(
  category: ExtractionCategory,
  run: AnalysisExtractionRun,
  update: ProgressUpdater,
) {
  let hasRequestedSection = false;
  let sectionArtifactCount = 0;
  for (const section of run.sections ?? []) {
    if (!isExtractionCategory(section.key)) continue;
    hasRequestedSection ||= section.key === category;
    sectionArtifactCount += section.artifactCount;
    update(section.key, {
      status: statusFromRun(section.status),
      scannedCount: section.scannedCount,
      artifactCount: section.artifactCount,
      timelineEventCount: section.timelineEventCount,
      warnings: section.warnings,
      error: undefined,
    });
  }
  if (!hasRequestedSection) {
    update(category, {
      status: statusFromRun(run.status),
      scannedCount: run.scannedCount,
      artifactCount: sectionArtifactCount,
      timelineEventCount: run.timelineEventCount,
      warnings: run.warnings,
      error: undefined,
    });
  }
}

function markFailed(category: ExtractionCategory, error: unknown, update: ProgressUpdater) {
  const message = errorMessage(error);
  update(category, { status: 'failed', error: message });
  if (category === 'LinuxArtifacts') {
    for (const progressCategory of LINUX_PROGRESS_CATEGORIES) {
      update(progressCategory, { status: 'failed', error: message });
    }
  }
}
