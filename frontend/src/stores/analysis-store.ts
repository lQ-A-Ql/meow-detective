import { create } from 'zustand';
import type {
  AnalysisExtractionProgressInfo,
  AnalysisExtractionProgressState,
} from '@/features/analysis/types';
import type {
  AnalysisTabKey,
  ExtractionCategory,
  LinuxAnalysisTabKey,
} from '@/features/analysis/types';
import { ANALYSIS_EXTRACTION_CATEGORIES } from '@/features/analysis/types';
export { isExtractionCategory } from '@/features/analysis/types';
export type {
  AnalysisPlatformView,
  AnalysisTabKey,
  ExtractionCategory,
  LinuxAnalysisTabKey,
} from '@/features/analysis/types';

function emptyProgress(): Omit<AnalysisExtractionProgressInfo, 'label'> {
  return {
    status: 'idle',
    scannedCount: 0,
    artifactCount: 0,
    timelineEventCount: 0,
    warnings: [],
    totalCandidateCount: 0,
    processedCandidateCount: 0,
    structuredCandidateCount: 0,
    unsupportedCandidateCount: 0,
    textFallbackCandidateCount: 0,
    warningCandidateCount: 0,
    checkpointHitCount: 0,
  };
}

function createDefaultProgress(): Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>> {
  return {
    Registry: emptyProgress(),
    BrowserHistory: emptyProgress(),
    Email: emptyProgress(),
    EventLogs: emptyProgress(),
    LinuxArtifacts: emptyProgress(),
    LinuxJournal: emptyProgress(),
    LinuxLogin: emptyProgress(),
    LinuxCommands: emptyProgress(),
    LinuxPackages: emptyProgress(),
    LinuxCron: emptyProgress(),
    LinuxSudo: emptyProgress(),
    LinuxSystemConfig: emptyProgress(),
    LinuxWebServices: emptyProgress(),
    LinuxMysqlServices: emptyProgress(),
  };
}

type AnalysisState = {
  extractionProgress: Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>>;
  extractionRunning: boolean;
  progressExpanded: boolean;
  activeTab: AnalysisTabKey;
  activeLinuxTab: LinuxAnalysisTabKey;
  selectedDataSourceId?: string;
  activePluginId?: string;

  setExtractionProgress: (
    progress: Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>>,
  ) => void;
  updateExtractionProgress: (
    category: ExtractionCategory,
    patch: Partial<Omit<AnalysisExtractionProgressInfo, 'label'>>,
  ) => void;
  resetExtractionProgress: () => void;
  setExtractionRunning: (running: boolean) => void;
  setProgressExpanded: (expanded: boolean) => void;
  setActiveTab: (tab: AnalysisTabKey) => void;
  setActiveLinuxTab: (tab: LinuxAnalysisTabKey) => void;
  setActivePluginId: (id?: string) => void;
  setSelectedDataSourceId: (id?: string) => void;
  reset: () => void;
};

const initialState: Omit<
  AnalysisState,
  | 'setExtractionProgress'
  | 'updateExtractionProgress'
  | 'resetExtractionProgress'
  | 'setExtractionRunning'
  | 'setProgressExpanded'
  | 'setActiveTab'
  | 'setActiveLinuxTab'
  | 'setActivePluginId'
  | 'setSelectedDataSourceId'
  | 'reset'
> = {
  extractionProgress: createDefaultProgress(),
  extractionRunning: false,
  progressExpanded: true,
  activeTab: 'system',
  activeLinuxTab: 'overview',
  selectedDataSourceId: undefined,
  activePluginId: undefined,
};

export const useAnalysisStore = create<AnalysisState>((set) => ({
  ...initialState,

  setExtractionProgress: (progress) => set({ extractionProgress: progress }),

  updateExtractionProgress: (category, patch) =>
    set((state) => ({
      extractionProgress: {
        ...state.extractionProgress,
        [category]: { ...state.extractionProgress[category], ...patch },
      },
    })),

  resetExtractionProgress: () =>
    set({ extractionProgress: createDefaultProgress() }),

  setExtractionRunning: (running) => set({ extractionRunning: running }),

  setProgressExpanded: (expanded) => set({ progressExpanded: expanded }),

  setActiveTab: (tab) => set({ activeTab: tab }),

  setActiveLinuxTab: (tab) => set({ activeLinuxTab: tab }),

  setActivePluginId: (id) => set({ activePluginId: id }),

  setSelectedDataSourceId: (id) => set({ selectedDataSourceId: id }),

  reset: () => set(initialState),
}));

export function statusFromRun(status: string): AnalysisExtractionProgressState {
  if (status === 'failed' || status === 'unavailable') {
    return 'failed';
  }
  if (status === 'partial') {
    return 'partial';
  }
  return 'success';
}

export function labeledProgress(
  progress: Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>>,
  t: (key: string) => string,
): Record<ExtractionCategory, AnalysisExtractionProgressInfo> {
  const result = {} as Record<ExtractionCategory, AnalysisExtractionProgressInfo>;
  for (const category of ANALYSIS_EXTRACTION_CATEGORIES) {
    result[category] = {
      ...progress[category],
      label: t(`analysis.extraction.${category}`),
    };
  }
  return result;
}
