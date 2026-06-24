import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FileBrowser } from './FileBrowser';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  fileTree: vi.fn(),
  fileRows: vi.fn(),
  fileChildren: vi.fn(),
  fileJumpContext: vi.fn(),
  fileViewer: vi.fn(),
  textPreview: vi.fn(),
  imagePreview: vi.fn(),
  mediaUrl: vi.fn(),
  extractMutate: vi.fn(),
  navigate: vi.fn(),
  selectionState: {
    selectedDirectoryId: 'root' as string | undefined,
    selectedFileId: 'video-1' as string | undefined,
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
  useDataSources: mocks.dataSources,
}));

vi.mock('@/features/files/hooks', () => ({
  useExtractFile: () => ({
    mutate: mocks.extractMutate,
    isPending: false,
  }),
  useFileChildrenPage: mocks.fileChildren,
  useFileJumpContext: mocks.fileJumpContext,
  useFileRowsPage: mocks.fileRows,
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
    isFetching: false,
    refetch: vi.fn(),
  };
}

const rootTree = [
  {
    id: 'root',
    name: 'Root',
    depth: 0,
    hasChildren: true,
    expanded: true,
    hidden: false,
    system: false,
    deleted: false,
  },
];

const videoFile = {
  id: 'video-1',
  parentId: 'root',
  path: '/evidence/video.mp4',
  name: 'video.mp4',
  entryType: 'file' as const,
  size: 8_000_000,
  ext: 'mp4',
  deleted: false,
  hidden: false,
  system: false,
};

const deletedFile = {
  ...videoFile,
  id: 'deleted-1',
  path: '/evidence/old.docx',
  name: 'old.docx',
  ext: 'docx',
  deleted: true,
};

const hiddenFile = {
  ...videoFile,
  id: 'hidden-1',
  path: '/evidence/System Volume Information',
  name: 'System Volume Information',
  entryType: 'directory' as const,
  size: undefined,
  ext: undefined,
  hidden: true,
  system: true,
};

const hiddenDeletedFile = {
  ...videoFile,
  id: 'hidden-deleted-1',
  path: '/evidence/.old.txt',
  name: '.old.txt',
  ext: 'txt',
  deleted: true,
  hidden: true,
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
    mocks.selectionState.setSelectedDirectoryId.mockImplementation((id?: string) => {
      mocks.selectionState.selectedDirectoryId = id;
    });
    mocks.selectionState.setSelectedFileId.mockImplementation((id?: string) => {
      mocks.selectionState.selectedFileId = id;
    });
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
    mocks.dataSources.mockReturnValue(queryState([]));
    mocks.fileTree.mockReturnValue(queryState(rootTree));
    mocks.fileRows.mockReturnValue(
      queryState({
        rows: [videoFile],
        totalCount: 1,
        offset: 0,
        limit: 500,
        truncated: false,
      }),
    );
    mocks.fileChildren.mockReturnValue(
      queryState({
        children: [],
        totalCount: 0,
        offset: 0,
        limit: 500,
        truncated: false,
      }),
    );
    mocks.fileJumpContext.mockReturnValue(queryState(null));
    mocks.fileViewer.mockReturnValue(
      queryState({
        handle: { handleId: 'file:video-1', size: videoFile.size, mime: 'video/mp4' },
        range: { kind: 'hex', lines: [] },
      }),
    );
    mocks.textPreview.mockReturnValue(queryState(null));
    mocks.imagePreview.mockReturnValue(queryState(null));
    mocks.mediaUrl.mockReturnValue(queryState(null));
  });

  it('shows large media controlled chunk fallback text and keeps extract available', () => {
    mocks.mediaUrl.mockReturnValue(
      queryState({
        handleId: 'file:video-1',
        mimeType: 'video/mp4',
        size: 8_000_000,
        canReadRanges: true,
        mode: 'rangeFallback',
        previewMode: 'rangeFallback',
        previewBytes: 0,
      }),
    );

    renderPage();

    expect(screen.getByText(/total=7\.6 MB \/ source=opaque handle/)).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: '\u63d0\u53d6\u6587\u4ef6' }));
    expect(mocks.extractMutate).toHaveBeenCalledWith(videoFile);
  });

  it('renders small media preview directly', () => {
    mocks.mediaUrl.mockReturnValue(
      queryState({
        url: 'data:video/mp4;base64,AAAA',
        handleId: 'file:video-1',
        mimeType: 'video/mp4',
        size: 1024,
        canReadRanges: true,
        mode: 'inline',
        previewMode: 'inline',
      }),
    );

    renderPage();

    expect(screen.getByTestId('video-viewer').textContent).toContain(
      'data:video/mp4;base64,AAAA',
    );
  });

  it('does not crash when selected media has no URL yet', () => {
    mocks.mediaUrl.mockReturnValue(
      queryState({
        mimeType: 'video/mp4',
        size: 0,
        canReadRanges: false,
      }),
    );

    renderPage();

    expect(screen.getByText('\u52a0\u8f7d\u89c6\u9891\u9884\u89c8...')).toBeDefined();
  });

  it('renders protocol media preview with scoped evidence URL', () => {
    mocks.mediaUrl.mockReturnValue(
      queryState({
        url: 'evidence-media://handle/ZmlsZTp2aWRlby0x',
        handleId: 'file:video-1',
        mimeType: 'video/mp4',
        size: 8_000_000,
        canReadRanges: true,
        mode: 'protocol',
        previewMode: 'protocol',
      }),
    );

    renderPage();

    expect(screen.getByTestId('video-viewer').textContent).toContain(
      'evidence-media://handle/ZmlsZTp2aWRlby0x',
    );
    expect(screen.getByTestId('video-viewer').textContent).not.toContain('C:\\');
    expect(screen.getByTestId('video-viewer').textContent).not.toContain('D:\\');
  });

  it('hides hidden and system files by default and reloads rows when enabled', async () => {
    const visibleRows = [videoFile, deletedFile];
    const allRows = [videoFile, deletedFile, hiddenFile];
    mocks.fileRows.mockImplementation((_parentId, _offset, _limit, showHidden) =>
      queryState({
        rows: showHidden ? allRows : visibleRows,
        totalCount: showHidden ? allRows.length : visibleRows.length,
        offset: 0,
        limit: 500,
        truncated: false,
      }),
    );

    renderPage();

    expect(mocks.fileTree).toHaveBeenCalledWith(false);
    expect(mocks.fileRows).toHaveBeenCalledWith('root', 0, 500, false, 'name', 'asc');
    expect(screen.queryByText('System Volume Information')).toBeNull();

    fireEvent.click(screen.getByTestId('file-visibility-toggle'));

    await waitFor(() => expect(mocks.fileTree).toHaveBeenLastCalledWith(true));
    await waitFor(() =>
      expect(mocks.fileRows).toHaveBeenLastCalledWith('root', 0, 500, true, 'name', 'asc'),
    );
    expect(screen.getByText('System Volume Information')).toBeDefined();
  });

  it('loads more tree children so hidden system directories remain reachable in large roots', async () => {
    Object.assign(mocks.selectionState, {
      selectedDirectoryId: undefined,
      selectedFileId: undefined,
    });
    const firstChildren = Array.from({ length: 500 }, (_, index) => ({
      id: `dir-${index}`,
      name: `Dir ${index}`,
      depth: 1,
      hasChildren: false,
      hidden: false,
      system: false,
      deleted: false,
    }));
    const nextChildren = [
      {
        id: 'svi',
        name: 'System Volume Information',
        depth: 1,
        hasChildren: true,
        hidden: true,
        system: true,
        deleted: false,
      },
    ];

    mocks.fileChildren.mockImplementation((parentId, offset, limit, showHidden) => {
      if (!showHidden) {
        return queryState({
          children: [],
          totalCount: 0,
          offset,
          limit,
          truncated: false,
        });
      }
      return queryState({
        children: offset === 0 ? firstChildren : nextChildren,
        totalCount: 501,
        offset,
        limit,
        truncated: offset === 0,
      });
    });
    mocks.fileRows.mockReturnValue(
      queryState({
        rows: [videoFile],
        totalCount: 1,
        offset: 0,
        limit: 500,
        truncated: false,
      }),
    );

    renderPage();

    fireEvent.click(screen.getByTestId('file-visibility-toggle'));

    await waitFor(() => expect(mocks.fileChildren).toHaveBeenCalledWith('root', 0, 500, true));
    expect(screen.queryByText('System Volume Information')).toBeNull();

    fireEvent.click(screen.getByTestId('load-more-tree-children'));

    await waitFor(() =>
      expect(mocks.fileChildren).toHaveBeenLastCalledWith('root', 500, 500, true),
    );
    expect(screen.getByText('System Volume Information')).toBeDefined();
  });

  it('renders deleted and hidden states as icon overlays', () => {
    Object.assign(mocks.selectionState, {
      selectedFileId: undefined,
    });
    mocks.fileRows.mockReturnValue(
      queryState({
        rows: [deletedFile, hiddenFile, hiddenDeletedFile],
        totalCount: 3,
        offset: 0,
        limit: 500,
        truncated: false,
      }),
    );

    renderPage();

    const deletedRow = screen.getByText('old.docx').closest('tr');
    const hiddenRow = screen.getByText('System Volume Information').closest('tr');
    const bothRow = screen.getByText('.old.txt').closest('tr');

    expect(deletedRow?.querySelector('[data-deleted="true"]')).toBeTruthy();
    expect(deletedRow?.querySelector('[data-hidden="true"]')).toBeNull();
    expect(hiddenRow?.querySelector('[data-hidden="true"]')).toBeTruthy();
    expect(hiddenRow?.querySelector('[data-deleted="true"]')).toBeNull();
    expect(bothRow?.querySelector('[data-deleted="true"]')).toBeTruthy();
    expect(bothRow?.querySelector('[data-hidden="true"]')).toBeTruthy();
  });

  it('formats partition root names in the tree and breadcrumb using shared partition display rules', () => {
    mocks.fileTree.mockReturnValue(
      queryState([
        {
          id: 'root',
          name: 'Partition 1 (NTFS)',
          depth: 0,
          hasChildren: true,
          expanded: true,
          hidden: false,
          system: false,
          deleted: false,
        },
      ]),
    );
    mocks.dataSources.mockReturnValue(
      queryState([
        {
          id: 'ds-1',
          name: 'Demo Source',
          kind: 'e01',
          sourcePath: 'E:/demo.E01',
          importedAt: '2026-06-01T10:00:00Z',
          partitions: [
            {
              index: 1,
              name: 'Basic data partition',
              kindLabel: 'Basic data',
              status: 'supported',
              offset: 0,
              length: 1024,
              filesystem: 'NTFS',
            },
          ],
        },
      ]),
    );

    renderPage();

    expect(screen.getAllByText('\u5206\u533a1\uff08NTFS\uff09').length).toBeGreaterThan(0);
    expect(screen.queryByText('Partition 1 (NTFS)')).toBeNull();
  });

  it('uses jump context to reveal off-page selected files and enable hidden visibility when needed', async () => {
    Object.assign(mocks.selectionState, {
      selectedDirectoryId: 'root',
      selectedFileId: 'hidden-offpage',
    });
    mocks.fileRows.mockImplementation((parentId, offset, _limit, showHidden) =>
      queryState({
        rows:
          parentId === 'dir-hidden' && offset === 500 && showHidden
            ? [
                {
                  id: 'hidden-offpage',
                  parentId: 'dir-hidden',
                  path: '/evidence/secret.dat',
                  name: 'secret.dat',
                  entryType: 'file',
                  deleted: false,
                  hidden: true,
                  system: false,
                },
              ]
            : [],
        totalCount: parentId === 'dir-hidden' && showHidden ? 501 : 0,
        offset,
        limit: 500,
        truncated: false,
      }),
    );
    mocks.fileJumpContext.mockImplementation((fileId, showHidden) =>
      queryState(
        fileId
          ? {
              target: {
                id: 'hidden-offpage',
                parentId: 'dir-hidden',
                path: '/evidence/secret.dat',
                name: 'secret.dat',
                entryType: 'file',
                deleted: false,
                hidden: true,
                system: false,
              },
              directory: {
                id: 'dir-hidden',
                parentId: 'root',
                path: '/evidence/hidden',
                name: 'hidden',
                entryType: 'directory',
                deleted: false,
                hidden: true,
                system: false,
              },
              ancestorDirectoryIds: ['root', 'dir-hidden'],
              rowOffset: 500,
              requiresShowHidden: !showHidden,
            }
          : null,
      ),
    );

    renderPage();

    await waitFor(() =>
      expect(mocks.fileJumpContext).toHaveBeenCalledWith(
        'hidden-offpage',
        false,
        500,
        'name',
        'asc',
      ),
    );
    await waitFor(() => expect(mocks.fileTree).toHaveBeenLastCalledWith(true));
    await waitFor(() =>
      expect(mocks.selectionState.setSelectedDirectoryId).toHaveBeenCalledWith('dir-hidden'),
    );
    await waitFor(() =>
      expect(mocks.fileRows).toHaveBeenLastCalledWith(
        'dir-hidden',
        500,
        500,
        true,
        'name',
        'asc',
      ),
    );
    expect(screen.getAllByText('secret.dat').length).toBeGreaterThan(0);
  });
});

