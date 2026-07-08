import { create } from 'zustand';
import type {
  AnalysisExtractionProgressInfo,
  AnalysisExtractionProgressState,
} from '@/features/analysis/components/AnalysisPanels';

export type AnalysisTabKey =
  | 'system'
  | 'evidence'
  | 'registry'
  | 'browser'
  | 'email'
  | 'eventlogs'
  | 'files'
  | 'report';

export type AnalysisPlatformView = 'windows' | 'linux';

export type LinuxAnalysisTabKey =
  | 'overview'
  | 'journal'
  | 'login'
  | 'commands'
  | 'packages'
  | 'cron'
  | 'sudo';

export type ExtractionCategory = 'Registry' | 'BrowserHistory' | 'Email' | 'EventLogs' | 'LinuxArtifacts';

const EXTRACTION_CATEGORIES: ExtractionCategory[] = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
  'LinuxArtifacts',
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
    LinuxArtifacts: emptyProgress(),
  };
}

type AnalysisState = {
  extractionProgress: Record<ExtractionCategory, Omit<AnalysisExtractionProgressInfo, 'label'>>;
  extractionRunning: boolean;
  progressExpanded: boolean;
  activePlatformView: AnalysisPlatformView;
  activeTab: AnalysisTabKey;
  activeLinuxTab: LinuxAnalysisTabKey;
  selectedDataSourceId?: string;

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
  setActivePlatformView: (view: AnalysisPlatformView) => void;
  setActiveTab: (tab: AnalysisTabKey) => void;
  setActiveLinuxTab: (tab: LinuxAnalysisTabKey) => void;
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
  | 'setActivePlatformView'
  | 'setActiveTab'
  | 'setActiveLinuxTab'
  | 'setSelectedDataSourceId'
  | 'reset'
> = {
  extractionProgress: createDefaultProgress(),
  extractionRunning: false,
  progressExpanded: true,
  activePlatformView: 'windows',
  activeTab: 'system',
  activeLinuxTab: 'overview',
  selectedDataSourceId: undefined,
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

  setActivePlatformView: (view) => set({ activePlatformView: view }),

  setActiveTab: (tab) => set({ activeTab: tab }),

  setActiveLinuxTab: (tab) => set({ activeLinuxTab: tab }),

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
  for (const category of EXTRACTION_CATEGORIES) {
    result[category] = {
      ...progress[category],
      label: t(`analysis.extraction.${category}`),
    };
  }
  return result;
}
