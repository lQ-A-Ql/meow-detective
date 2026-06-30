import { create } from 'zustand';
import type {
  AnalysisExtractionProgressInfo,
  AnalysisExtractionProgressState,
} from '@/components/analysis/AnalysisPanels';

export type AnalysisTabKey =
  | 'system'
  | 'evidence'
  | 'registry'
  | 'browser'
  | 'email'
  | 'eventlogs'
  | 'files'
  | 'report';

export type ExtractionCategory = 'Registry' | 'BrowserHistory' | 'Email' | 'EventLogs';

const EXTRACTION_CATEGORIES: ExtractionCategory[] = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
];

function emptyProgress(): Omit<AnalysisExtractionProgressInfo, 'label'> {
  return {
    status: 'idle',
    scannedCount: 0,
    artifactCount: 0,
    timelineEventCount: 0,
    warnings: [],
  };
}

function createDefaultProgress(): Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>> {
  return {
    Registry: emptyProgress(),
    BrowserHistory: emptyProgress(),
    Email: emptyProgress(),
    EventLogs: emptyProgress(),
  };
}

type AnalysisState = {
  extractionProgress: Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>>;
  extractionRunning: boolean;
  progressExpanded: boolean;
  activeTab: AnalysisTabKey;

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
  | 'reset'
> = {
  extractionProgress: createDefaultProgress(),
  extractionRunning: false,
  progressExpanded: true,
  activeTab: 'system',
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
  for (const category of EXTRACTION_CATEGORIES) {
    result[category] = {
      ...progress[category],
      label: t(`analysis.extraction.${category}`),
    };
  }
  return result;
}
