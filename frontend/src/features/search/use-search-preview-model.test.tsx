import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useFilePreview: vi.fn(() => ({
    fileHandle: undefined,
    previewKind: undefined,
    viewer: undefined,
    setJumpOffsetInput: vi.fn(),
    jumpToOffset: vi.fn(),
    loadNextRange: vi.fn(),
    loadPreviousRange: vi.fn(),
    textPreview: undefined,
    imagePreview: undefined,
    mediaUrl: undefined,
    documentPreview: undefined,
    previewError: null,
    onRetryPreview: undefined,
  })),
}));

vi.mock('@/features/files/hooks/use-file-preview', () => ({
  useFilePreview: mocks.useFilePreview,
}));

import { useSearchPreviewModel } from './use-search-preview-model';

const textHit = {
  fileId: 'file-1',
  dataSourceId: 'source-1',
  dataSourceName: '检材2.E01',
  name: 'report.txt',
  path: 'Users/alice/report.txt',
  entryType: 'file',
  extension: 'txt',
  size: 12,
  modifiedAt: '2026-07-29T12:00:00Z',
  deleted: false,
  hidden: false,
  system: false,
  encrypted: false,
};

describe('useSearchPreviewModel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('keeps preview content disabled until a hit is clicked and disables it on close', () => {
    const { result } = renderHook(() => useSearchPreviewModel());

    expect(mocks.useFilePreview).toHaveBeenLastCalledWith(
      expect.objectContaining({ selectedFile: undefined }),
    );
    expect(result.current.open).toBe(false);

    act(() => result.current.openHit(textHit));
    expect(result.current.open).toBe(true);
    expect(result.current.viewerTab).toBe('text');
    expect(mocks.useFilePreview).toHaveBeenLastCalledWith(
      expect.objectContaining({
        selectedFile: expect.objectContaining({ id: 'file-1', entryType: 'file' }),
      }),
    );

    act(() => result.current.onOpenChange(false));
    expect(result.current.open).toBe(false);
    expect(mocks.useFilePreview).toHaveBeenLastCalledWith(
      expect.objectContaining({ selectedFile: undefined }),
    );
  });

  it('opens directory hits as index-backed metadata without enabling file content', () => {
    const { result } = renderHook(() => useSearchPreviewModel());

    act(() => result.current.openHit({ ...textHit, fileId: 'dir-1', entryType: 'directory' }));

    expect(result.current.viewerTab).toBe('metadata');
    expect(result.current.selectedFile?.entryType).toBe('directory');
    expect(mocks.useFilePreview).toHaveBeenLastCalledWith(
      expect.objectContaining({ selectedFile: undefined }),
    );
  });
});
