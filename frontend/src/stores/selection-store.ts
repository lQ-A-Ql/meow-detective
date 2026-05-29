import { create } from 'zustand';

type SelectionState = {
  selectedDirectoryId?: string;
  selectedFileId?: string;
  selectedSearchHitId?: string;
  selectedTimelineId?: string;
  selectedArtifactFamily: string;
  selectedArtifactId?: string;
  setSelectedDirectoryId: (id?: string) => void;
  setSelectedFileId: (id?: string) => void;
  setSelectedSearchHitId: (id?: string) => void;
  setSelectedTimelineId: (id?: string) => void;
  setSelectedArtifactFamily: (family: string) => void;
  setSelectedArtifactId: (id?: string) => void;
};

export const useSelectionStore = create<SelectionState>((set) => ({
  selectedDirectoryId: undefined,
  selectedFileId: undefined,
  selectedSearchHitId: undefined,
  selectedTimelineId: undefined,
  selectedArtifactFamily: 'LNK',
  selectedArtifactId: undefined,
  setSelectedDirectoryId: (id) => set({ selectedDirectoryId: id }),
  setSelectedFileId: (id) => set({ selectedFileId: id }),
  setSelectedSearchHitId: (id) => set({ selectedSearchHitId: id }),
  setSelectedTimelineId: (id) => set({ selectedTimelineId: id }),
  setSelectedArtifactFamily: (family) => set({ selectedArtifactFamily: family }),
  setSelectedArtifactId: (id) => set({ selectedArtifactId: id }),
}));
