import { beforeEach, describe, expect, it } from 'vitest';
import { useSelectionStore } from '@/stores/selection-store';

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

  it('updates directory, search-hit, timeline and artifact selections independently', () => {
    const store = useSelectionStore.getState();

    store.setSelectedDirectoryId('dir-1');
    store.setSelectedSearchHitId('hit-1');
    store.setSelectedTimelineId('event-1');
    store.setSelectedArtifactId('artifact-1');

    expect(useSelectionStore.getState()).toMatchObject({
      selectedDirectoryId: 'dir-1',
      selectedSearchHitId: 'hit-1',
      selectedTimelineId: 'event-1',
      selectedArtifactId: 'artifact-1',
    });
  });

  it('clears every selection when passed undefined', () => {
    const store = useSelectionStore.getState();
    store.setSelectedDirectoryId('dir-1');
    store.setSelectedFileId('file-1');
    store.setSelectedSearchHitId('hit-1');
    store.setSelectedTimelineId('event-1');
    store.setSelectedArtifactId('artifact-1');

    store.setSelectedDirectoryId(undefined);
    store.setSelectedFileId(undefined);
    store.setSelectedSearchHitId(undefined);
    store.setSelectedTimelineId(undefined);
    store.setSelectedArtifactId(undefined);

    expect(useSelectionStore.getState()).toMatchObject({
      selectedDirectoryId: undefined,
      selectedFileId: undefined,
      selectedSearchHitId: undefined,
      selectedTimelineId: undefined,
      selectedArtifactId: undefined,
    });
  });
});
