import type { DataSourceSummary } from '@/types/models';
import {
  ANALYSIS_EXTRACTION_CATEGORIES,
  isExtractionCategory,
  type AnalysisExtractionProgressInfo,
  type AnalysisExtractionProgressState,
  type AnalysisTabKey,
  type AndroidAnalysisTabKey,
  type ExtractionCategory,
  type LinuxAnalysisTabKey,
} from '@/types/analysisUi';

export {
  ANALYSIS_EXTRACTION_CATEGORIES,
  isExtractionCategory,
  type AnalysisExtractionProgressInfo,
  type AnalysisExtractionProgressState,
  type AnalysisTabKey,
  type AndroidAnalysisTabKey,
  type ExtractionCategory,
  type LinuxAnalysisTabKey,
};

export type AnalysisPlatformView = DataSourceSummary['platform'];

const WINDOWS_EXTRACTION_CATEGORIES: ExtractionCategory[] = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
];

export const LINUX_PROGRESS_CATEGORIES: ExtractionCategory[] = [
  'LinuxJournal',
  'LinuxLogin',
  'LinuxCommands',
  'LinuxPackages',
  'LinuxCron',
  'LinuxSudo',
  'LinuxSystemConfig',
  'LinuxWebServices',
  'LinuxMysqlServices',
];

export const EXTRACTION_CATEGORIES_BY_PLATFORM: Record<AnalysisPlatformView, ExtractionCategory[]> = {
  windows: WINDOWS_EXTRACTION_CATEGORIES,
  linux: ['LinuxArtifacts'],
  android: [],
};

export const PROGRESS_CATEGORIES_BY_PLATFORM: Record<AnalysisPlatformView, ExtractionCategory[]> = {
  windows: WINDOWS_EXTRACTION_CATEGORIES,
  linux: LINUX_PROGRESS_CATEGORIES,
  android: [],
};

export interface AnalysisOperationToken {
  key: string;
  generation: number;
}

/** Invalidates source-bound async continuations without cancelling backend work. */
export class AnalysisSourceEpoch {
  private generation = 0;
  private activeOperations = 0;

  constructor(private key?: string) {}

  sync(nextKey?: string) {
    if (this.key === nextKey) {
      return false;
    }
    this.key = nextKey;
    this.generation += 1;
    this.activeOperations = 0;
    return true;
  }

  begin(expectedKey?: string): AnalysisOperationToken | undefined {
    if (!expectedKey || this.key !== expectedKey) {
      return undefined;
    }
    this.activeOperations += 1;
    return { key: expectedKey, generation: this.generation };
  }

  isCurrent(operation: AnalysisOperationToken) {
    return this.key === operation.key && this.generation === operation.generation;
  }

  finish(operation: AnalysisOperationToken) {
    if (operation.generation === this.generation && operation.key === this.key) {
      this.activeOperations = Math.max(0, this.activeOperations - 1);
    }
  }

  get isBusy() {
    return this.activeOperations > 0;
  }
}

export function analysisSourceContextKey(
  caseId: string | undefined,
  source: { id: string; platform: AnalysisPlatformView } | undefined,
) {
  return source ? `${caseId ?? ''}\u0000${source.id}\u0000${source.platform}` : undefined;
}
