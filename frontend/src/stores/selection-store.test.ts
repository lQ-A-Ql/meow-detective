import { beforeEach, describe, expect, it } from 'vitest';
import { useSelectionStore } from './selection-store';

describe('selection-store', () => {
  beforeEach(() => {
    useSelectionStore.setState({
      selectedDirectoryId: undefined,
      selectedFileId: undefined,
      selectedSearchHitId: undefined,
      selectedTimelineId: undefined,
      selectedArtifactFamily: 'LNK',
      selectedArtifactId: undefined,
    });
  });

  it('initial state has null selections', () => {
    const state = useSelectionStore.getState();

    expect(state.selectedDirectoryId).toBeUndefined();
    expect(state.selectedFileId).toBeUndefined();
    expect(state.selectedSearchHitId).toBeUndefined();
    expect(state.selectedTimelineId).toBeUndefined();
    expect(state.selectedArtifactId).toBeUndefined();
  });

  it('setSelectedFileId updates file selection', () => {
    useSelectionStore.getState().setSelectedFileId('file-123');

    expect(useSelectionStore.getState().selectedFileId).toBe('file-123');
  });

  it('setSelectedArtifactFamily updates family', () => {
    expect(useSelectionStore.getState().selectedArtifactFamily).toBe('LNK');

    useSelectionStore.getState().setSelectedArtifactFamily('Prefetch');

    expect(useSelectionStore.getState().selectedArtifactFamily).toBe('Prefetch');
  });
});
