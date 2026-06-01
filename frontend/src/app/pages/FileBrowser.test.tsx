import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FileBrowser } from './FileBrowser';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  fileTree: vi.fn(),
  fileRows: vi.fn(),
  fileChildren: vi.fn(),
  fileViewer: vi.fn(),
  textPreview: vi.fn(),
  imagePreview: vi.fn(),
  mediaUrl: vi.fn(),
  extractMutate: vi.fn(),
  navigate: vi.fn(),
  selectionState: {
    selectedDirectoryId: 'root',
    selectedFileId: 'video-1',
    selectedSearchHitId: undefined as string | undefined,
    selectedTimelineId: undefined as string | undefined,
    selectedArtifactFamily: 'LNK',
    selectedArtifactId: undefined as string | undefined,
    setSelectedDirectoryId: vi.fn(),
    setSelectedFileId: vi.fn(),
    setSelectedSearchHitId: vi.fn(),
    setSelectedTimelineId: vi.fn(),
    setSelectedArtifactFamily: vi.fn(),
    setSelectedArtifactId: vi.fn(),
  },
  uiState: {
    currentPage: 'files',
    drawerOpen: false,
    viewerTab: 'preview',
    rightPanelTab: 'details',
    globalSearchQuery: '',
    fileSortKey: 'name',
    fileSortDirection: 'asc',
    setCurrentPage: vi.fn(),
    setDrawerOpen: vi.fn(),
    toggleDrawer: vi.fn(),
    setViewerTab: vi.fn(),
    setRightPanelTab: vi.fn(),
    setGlobalSearchQuery: vi.fn(),
    setFileSortKey: vi.fn(),
    toggleFileSortDirection: vi.fn(),
  },
}));

vi.mock('react-router', () => ({
  useNavigate: () => mocks.navigate,
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
}));

vi.mock('@/features/files/hooks', () => ({
  useExtractFile: () => ({
    mutate: mocks.extractMutate,
    isPending: false,
  }),
  useFileChildren: mocks.fileChildren,
  useFileRows: mocks.fileRows,
  useFileTree: mocks.fileTree,
  useFileViewer: mocks.fileViewer,
  useTextPreview: mocks.textPreview,
  useImagePreview: mocks.imagePreview,
  useMediaUrl: mocks.mediaUrl,
}));

vi.mock('@/hooks/use-resizable-panel', () => ({
  useResizablePanel: () => ({
    width: 224,
    isResizing: false,
    onResizeStart: vi.fn(),
  }),
}));

vi.mock('@/stores/selection-store', () => {
  const useSelectionStore = Object.assign(
    vi.fn((selector) => selector(mocks.selectionState)),
    {
      getState: () => mocks.selectionState,
      setState: (patch: Record<string, unknown>) => {
        Object.assign(mocks.selectionState, patch);
      },
    },
  );
  return { useSelectionStore };
});

vi.mock('@/stores/ui-store', () => ({
  useUiStore: vi.fn((selector) => selector(mocks.uiState)),
}));

vi.mock('@/components/viewers/VideoViewer', () => ({
  VideoViewer: ({ src, fileName }: { src: string; fileName?: string }) => (
    <div data-testid="video-viewer">video:{fileName}:{src}</div>
  ),
}));

vi.mock('@/components/viewers/AudioViewer', () => ({
  AudioViewer: ({ src, fileName }: { src: string; fileName?: string }) => (
    <div data-testid="audio-viewer">audio:{fileName}:{src}</div>
  ),
}));

function queryState(data: unknown) {
  return {
    data,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
  };
}

const rootTree = [
  { id: 'root', name: 'Root', depth: 0, hasChildren: true, expanded: true },
];

const videoFile = {
  id: 'video-1',
  path: '/evidence/video.mp4',
  name: 'video.mp4',
  entryType: 'file' as const,
  size: 8_000_000,
  ext: 'mp4',
  deleted: false,
};

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <FileBrowser />
    </QueryClientProvider>,
  );
}

describe('FileBrowser media preview', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(mocks.selectionState, {
      selectedDirectoryId: 'root',
      selectedFileId: 'video-1',
      selectedSearchHitId: undefined,
      selectedTimelineId: undefined,
      selectedArtifactFamily: 'LNK',
      selectedArtifactId: undefined,
    });
    Object.assign(mocks.uiState, {
      viewerTab: 'preview',
      fileSortKey: 'name',
      fileSortDirection: 'asc',
    });

    mocks.currentCase.mockReturnValue(queryState({ id: 'case-1', name: 'Case 1' }));
    mocks.fileTree.mockReturnValue(queryState(rootTree));
    mocks.fileRows.mockReturnValue(queryState([videoFile]));
    mocks.fileChildren.mockReturnValue(queryState([]));
    mocks.fileViewer.mockReturnValue(queryState({
      handle: { handleId: 'file:video-1', size: videoFile.size, mime: 'video/mp4' },
      range: { kind: 'hex', lines: [] },
    }));
    mocks.textPreview.mockReturnValue(queryState(null));
    mocks.imagePreview.mockReturnValue(queryState(null));
    mocks.mediaUrl.mockReturnValue(queryState(null));
  });

  it('shows large media controlled chunk fallback text and keeps extract available', () => {
    mocks.mediaUrl.mockReturnValue(queryState({
      handleId: 'file:video-1',
      mimeType: 'video/mp4',
      size: 8_000_000,
      canReadRanges: true,
      previewMode: 'range',
      previewBytes: 0,
    }));

    renderPage();

    expect(screen.getByText('大视频使用受控分块预览')).toBeDefined();
    expect(screen.getByText(/完整播放需要先使用右侧“提取文件”导出后在本机播放器查看/)).toBeDefined();
    expect(screen.getByText(/total=7\.6 MB \/ source=opaque handle/)).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: '提取文件' }));
    expect(mocks.extractMutate).toHaveBeenCalledWith(videoFile);
  });

  it('renders small media preview directly', () => {
    mocks.mediaUrl.mockReturnValue(queryState({
      url: 'data:video/mp4;base64,AAAA',
      handleId: 'file:video-1',
      mimeType: 'video/mp4',
      size: 1024,
      canReadRanges: true,
      previewMode: 'inline',
    }));

    renderPage();

    expect(screen.getByTestId('video-viewer').textContent).toContain('data:video/mp4;base64,AAAA');
    expect(screen.queryByText(/受控分块预览/)).toBeNull();
  });

  it('does not crash when selected media has no URL yet', () => {
    mocks.mediaUrl.mockReturnValue(queryState({
      mimeType: 'video/mp4',
      size: 0,
      canReadRanges: false,
    }));

    renderPage();

    expect(screen.getByText('加载视频预览...')).toBeDefined();
    expect(screen.getByRole('button', { name: '提取文件' })).toBeDefined();
  });
});
