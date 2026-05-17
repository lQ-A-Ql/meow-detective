import { create } from 'zustand';

type SelectionState = {
  selectedFileId?: string;
  selectedSearchHitId?: string;
  selectedTimelineId?: string;
  selectedArtifactFamily: string;
  selectedArtifactId?: string;
  setSelectedFileId: (id?: string) => void;
  setSelectedSearchHitId: (id?: string) => void;
  setSelectedTimelineId: (id?: string) => void;
  setSelectedArtifactFamily: (family: string) => void;
  setSelectedArtifactId: (id?: string) => void;
};

export const useSelectionStore = create<SelectionState>((set) => ({
  selectedFileId: 'file-cmd-exe',
  selectedSearchHitId: 'search-hit-1',
  selectedTimelineId: 'timeline-2',
  selectedArtifactFamily: 'LNK',
  selectedArtifactId: 'artifact-2',
  setSelectedFileId: (id) => set({ selectedFileId: id }),
  setSelectedSearchHitId: (id) => set({ selectedSearchHitId: id }),
  setSelectedTimelineId: (id) => set({ selectedTimelineId: id }),
  setSelectedArtifactFamily: (family) => set({ selectedArtifactFamily: family }),
  setSelectedArtifactId: (id) => set({ selectedArtifactId: id }),
}));
