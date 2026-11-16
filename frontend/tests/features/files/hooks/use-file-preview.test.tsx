import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ApiErrorDto } from '@/types/models';

const mocks = vi.hoisted(() => ({
  useFileHandle: vi.fn(),
  useFileViewer: vi.fn(),
  useTextPreview: vi.fn(),
  useImagePreview: vi.fn(),
  useImageMetadata: vi.fn(),
  useMediaUrl: vi.fn(),
  useDocumentPreview: vi.fn(),
}));

vi.mock('@/features/files/hooks', () => mocks);

import { useFilePreview } from '@/features/files/hooks/use-file-preview';
import type { FileEntryRow } from '@/types/models';

const inert = () => ({
  data: undefined,
  error: null,
  isLoading: false,
  isError: false,
  refetch: vi.fn(),
  setJumpOffsetInput: vi.fn(),
  jumpToOffset: vi.fn(),
  loadNextRange: vi.fn(),
  loadPreviousRange: vi.fn(),
});

const selectedFile: FileEntryRow = {
  id: 'file-1',
  path: '/evidence/picture.tif',
  name: 'picture.tif',
  entryType: 'file',
  size: 1024,
  deleted: false,
  hidden: false,
  system: false,
  readOnly: false,
  archive: false,
};

const error: ApiErrorDto = { code: 'E', message: 'boom', category: 'internal', recoverable: true };

function setQueries(options: {
  previewKind: 'image' | 'imageMetadataOnly';
  size?: number;
  handleError?: ApiErrorDto | null;
  documentError?: ApiErrorDto | null;
}) {
  mocks.useFileHandle.mockReturnValue({
    ...inert(),
    data: { handleId: 'handle-1', size: options.size ?? selectedFile.size, previewKind: options.previewKind },
    error: options.handleError ?? null,
  });
  for (const hook of [
    mocks.useFileViewer,
    mocks.useTextPreview,
    mocks.useImagePreview,
    mocks.useImageMetadata,
    mocks.useMediaUrl,
  ]) {
    hook.mockReturnValue(inert());
  }
  mocks.useDocumentPreview.mockReturnValue({ ...inert(), error: options.documentError ?? null });
}

describe('useFilePreview image gating', () => {
  it('enables metadata and skips image preview for metadata-only images', () => {
    setQueries({ previewKind: 'imageMetadataOnly' });
    const { result } = renderHook(() => useFilePreview({ selectedFile, viewerTab: 'preview', caseId: 'case-1' }));

    expect(result.current.previewKind).toBe('imageMetadataOnly');
    expect(result.current.imageMetadataOnly).toBe(true);
    expect(result.current.imagePreviewOversized).toBe(false);
    expect(mocks.useImageMetadata).toHaveBeenCalledWith('file-1', true, 'case-1');
    expect(mocks.useImagePreview).toHaveBeenCalledWith('file-1', false, 'case-1');
    expect(result.current.previewError).toBeNull();
  });

  it('does not request image preview for files above the inline limit', () => {
    setQueries({ previewKind: 'image', size: 6 * 1024 * 1024 });
    const { result } = renderHook(() =>
      useFilePreview({
        selectedFile: { ...selectedFile, size: 6 * 1024 * 1024 },
        viewerTab: 'preview',
        caseId: 'case-1',
      }),
    );

    expect(result.current.imagePreviewOversized).toBe(true);
    expect(mocks.useImagePreview).toHaveBeenCalledWith('file-1', false, 'case-1');
    expect(mocks.useImageMetadata).toHaveBeenCalledWith('file-1', true, 'case-1');
  });

  it('requests image preview for files within the inline limit', () => {
    setQueries({ previewKind: 'image' });
    renderHook(() => useFilePreview({ selectedFile, viewerTab: 'preview', caseId: 'case-1' }));

    expect(mocks.useImagePreview).toHaveBeenCalledWith('file-1', true, 'case-1');
  });

  it('routes metadata-only errors to the handle instead of the document preview', () => {
    setQueries({ previewKind: 'imageMetadataOnly', documentError: error });
    const { result } = renderHook(() => useFilePreview({ selectedFile, viewerTab: 'preview', caseId: 'case-1' }));

    expect(result.current.previewError).toBeNull();
  });
});
