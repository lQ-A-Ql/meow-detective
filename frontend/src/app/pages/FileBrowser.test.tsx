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
  fileHandle: vi.fn(),
  fileViewer: vi.fn(),
  textPreview: vi.fn(),
  imagePreview: vi.fn(),
  mediaUrl: vi.fn(),
  documentPreview: vi.fn(),
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
  useFileHandle: mocks.fileHandle,
  useFileViewer: mocks.fileViewer,
  useTextPreview: mocks.textPreview,
  useImagePreview: mocks.imagePreview,
  useMediaUrl: mocks.mediaUrl,
  useDocumentPreview: mocks.documentPreview,
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
      selectedDirectoryId: undefined,
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
    mocks.fileHandle.mockReturnValue(
      queryState({
        handleId: 'file:video-1',
        size: videoFile.size,
        mime: 'video/mp4',
      }),
    );
    mocks.fileViewer.mockReturnValue(
      {
        ...queryState({
          handle: { handleId: 'file:video-1', size: videoFile.size, mime: 'video/mp4' },
          mode: 'chunked',
          chunkSize: 64 * 1024,
          fileSize: videoFile.size,
          lines: [],
          rawBytes: [],
          baseOffset: 0,
          loadedRanges: [],
          activeOffset: 0,
          jumpOffsetInput: '0x0',
          isFullyLoaded: false,
          isLoadingMore: false,
          hasMoreBefore: false,
          hasMoreAfter: true,
        }),
        setJumpOffsetInput: vi.fn(),
        jumpToOffset: vi.fn(),
        loadNextRange: vi.fn(),
        loadPreviousRange: vi.fn(),
      },
    );
    mocks.textPreview.mockReturnValue(queryState(null));
    mocks.imagePreview.mockReturnValue(queryState(null));
    mocks.mediaUrl.mockReturnValue(queryState(null));
    mocks.documentPreview.mockReturnValue(queryState(null));
  });

  it('enables only the hex preview chain on the hex tab', () => {
    Object.assign(mocks.uiState, {
      viewerTab: 'hex',
    });

    renderPage();

    expect(mocks.fileViewer).toHaveBeenCalledWith('video-1', true);
    expect(mocks.textPreview).toHaveBeenCalledWith('video-1', false);
    expect(mocks.imagePreview).toHaveBeenCalledWith('video-1', false);
    expect(mocks.mediaUrl).toHaveBeenCalledWith('video-1', false);
    expect(mocks.fileHandle).toHaveBeenCalledWith('video-1', false);
  });

  it('enables only the text preview chain on the text tab', () => {
    Object.assign(mocks.uiState, {
      viewerTab: 'text',
    });

    renderPage();

    expect(mocks.fileViewer).toHaveBeenCalledWith('video-1', false);
    expect(mocks.textPreview).toHaveBeenCalledWith('video-1', true);
    expect(mocks.imagePreview).toHaveBeenCalledWith('video-1', false);
    expect(mocks.mediaUrl).toHaveBeenCalledWith('video-1', false);
    expect(mocks.fileHandle).toHaveBeenCalledWith('video-1', false);
  });

  it('enables only the matching media preview chain on the preview tab', () => {
    renderPage();

    expect(mocks.fileViewer).toHaveBeenCalledWith('video-1', false);
    expect(mocks.textPreview).toHaveBeenCalledWith('video-1', false);
    expect(mocks.imagePreview).toHaveBeenCalledWith('video-1', false);
    expect(mocks.mediaUrl).toHaveBeenCalledWith('video-1', true);
    expect(mocks.fileHandle).toHaveBeenCalledWith('video-1', false);
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

  it('does not render paging controls in hex preview mode', () => {
    Object.assign(mocks.uiState, {
      viewerTab: 'hex',
    });
    mocks.fileViewer.mockReturnValue({
      ...queryState({
        handle: { handleId: 'file:hex-1', size: 2048, mime: 'image/png' },
        mode: 'full',
        chunkSize: 64 * 1024,
        fileSize: 2048,
        lines: [
          '00000000  89 50 4E 47 0D 0A 1A 0A  00 00 00 0D 49 48 44 52',
          '00000010  00 00 07 80 00 04 38 08  06 00 00 00 E8 D3 C1',
        ],
        rawBytes: [0x89, 0x50, 0x4E, 0x47],
        baseOffset: 0,
        loadedRanges: [{ start: 0, end: 2048 }],
        activeOffset: 0,
        jumpOffsetInput: '0x0',
        isFullyLoaded: true,
        isLoadingMore: false,
        hasMoreBefore: false,
        hasMoreAfter: false,
      }),
      setJumpOffsetInput: vi.fn(),
      jumpToOffset: vi.fn(),
      loadNextRange: vi.fn(),
      loadPreviousRange: vi.fn(),
    });

    renderPage();

    expect(screen.queryByText('上一页')).toBeNull();
    expect(screen.queryByText('下一页')).toBeNull();
    expect(screen.queryByText(/显示第/)).toBeNull();
    expect(screen.getByText('完整 Hex 预览')).toBeDefined();
    expect(screen.queryByRole('button', { name: '跳转' })).toBeNull();
  });

  it('shows hidden and system files by default and reloads rows when disabled', async () => {
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

    expect(mocks.fileTree).toHaveBeenCalledWith(true);
    expect(mocks.fileRows).toHaveBeenCalledWith('root', 0, 500, true, 'name', 'asc');
    expect(screen.getByText('System Volume Information')).toBeDefined();

    fireEvent.click(screen.getByTestId('file-visibility-toggle'));

    await waitFor(() => expect(mocks.fileTree).toHaveBeenLastCalledWith(false));
    await waitFor(() =>
      expect(mocks.fileRows).toHaveBeenLastCalledWith('root', 0, 500, false, 'name', 'asc'),
    );
    expect(screen.queryByText('System Volume Information')).toBeNull();
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

  it('wraps partition roots under data source parent nodes when data sources exist', async () => {
    mocks.fileTree.mockReturnValue(
      queryState([
        {
          id: 'root-ds-1',
          name: 'Partition 1 (NTFS)',
          depth: 0,
          hasChildren: true,
          dataSourceId: 'ds-1',
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
          platform: 'windows',
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

    // Data source nodes are rendered at the top of the file tree
    const dsElements = await screen.findAllByText(/Demo Source/);
    expect(dsElements.length).toBeGreaterThan(0);
    // The raw partition root name is never shown \u2014 only the formatted version
    expect(screen.queryByText('Partition 1 (NTFS)')).toBeNull();
  });

  it('keeps roots isolated per data source and does not fetch rows for synthetic nodes', async () => {
    Object.assign(mocks.selectionState, {
      selectedDirectoryId: undefined,
      selectedFileId: undefined,
    });
    mocks.fileTree.mockReturnValue(
      queryState([
        {
          id: 'root-ds-1',
          name: 'Partition 1 (NTFS)',
          depth: 0,
          hasChildren: true,
          dataSourceId: 'ds-1',
          expanded: false,
          hidden: false,
          system: false,
          deleted: false,
        },
        {
          id: 'root-ds-2',
          name: 'Partition 2 (FAT)',
          depth: 0,
          hasChildren: true,
          dataSourceId: 'ds-2',
          expanded: false,
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
          name: 'Windows Source',
          kind: 'e01',
          sourcePath: 'E:/windows.E01',
          importedAt: '2026-06-01T10:00:00Z',
          platform: 'windows',
          partitions: [{ index: 1, name: 'Windows', kindLabel: 'NTFS', status: 'supported', offset: 0, length: 1024, filesystem: 'NTFS' }],
        },
        {
          id: 'ds-2',
          name: 'Linux Source',
          kind: 'raw',
          sourcePath: 'E:/linux.raw',
          importedAt: '2026-06-02T10:00:00Z',
          platform: 'linux',
          partitions: [{ index: 2, name: 'Linux', kindLabel: 'FAT', status: 'supported', offset: 1024, length: 2048, filesystem: 'FAT' }],
        },
      ]),
    );

    renderPage();

    // FileTreeDataSourceNode rows are the only elements with role="button" whose
    // accessible name includes the data source name — this disambiguates them from
    // the breadcrumb and inspector panel, which also render the same source name.
    const windowsTreeNode = await screen.findByRole('button', { name: /Windows Source/ });
    const linuxTreeNode = screen.getByRole('button', { name: /Linux Source/ });
    expect(windowsTreeNode).toBeDefined();
    expect(linuxTreeNode).toBeDefined();

    // ds-1 (Windows Source) auto-expands first; only its own root should show.
    await waitFor(() => expect(screen.getByText('分区1（NTFS）')).toBeDefined());
    expect(screen.queryByText('分区2（FAT）')).toBeNull();
    expect(mocks.fileRows).toHaveBeenCalledWith(undefined, 0, 500, true, 'name', 'asc');

    fireEvent.click(windowsTreeNode); // collapse ds-1
    fireEvent.click(linuxTreeNode); // expand ds-2
    await waitFor(() => expect(screen.getByText('分区2（FAT）')).toBeDefined());
    expect(screen.queryByText('分区1（NTFS）')).toBeNull();
  });

  it('formats partition roots with their own data source metadata when sources share partition indexes', async () => {
    Object.assign(mocks.selectionState, {
      selectedDirectoryId: undefined,
      selectedFileId: undefined,
    });
    mocks.fileTree.mockReturnValue(
      queryState([
        {
          id: 'root-ds-win-p1',
          name: 'Partition 1 (NTFS)',
          depth: 0,
          hasChildren: true,
          dataSourceId: 'ds-win',
          expanded: false,
          hidden: false,
          system: false,
          deleted: false,
        },
        {
          id: 'root-ds-linux-p1',
          name: 'Partition 1 (XFS)',
          depth: 0,
          hasChildren: true,
          dataSourceId: 'ds-linux',
          expanded: false,
          hidden: false,
          system: false,
          deleted: false,
        },
      ]),
    );
    mocks.dataSources.mockReturnValue(
      queryState([
        {
          id: 'ds-win',
          name: 'Windows Source',
          kind: 'e01',
          sourcePath: 'D:/evidence/windows.E01',
          importedAt: '2026-06-01T10:00:00Z',
          platform: 'windows',
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
        {
          id: 'ds-linux',
          name: 'Linux Source',
          kind: 'e01',
          sourcePath: 'D:/evidence/linux.E01',
          importedAt: '2026-06-02T10:00:00Z',
          platform: 'linux',
          partitions: [
            {
              index: 1,
              name: 'Linux root LV',
              kindLabel: 'Linux LVM',
              status: 'supported',
              offset: 2048,
              length: 4096,
              filesystem: 'XFS',
            },
          ],
        },
      ]),
    );

    renderPage();

    const windowsTreeNode = await screen.findByRole('button', { name: /Windows Source/ });
    const linuxTreeNode = screen.getByRole('button', { name: /Linux Source/ });

    await waitFor(() => expect(screen.getByText('分区1（NTFS）')).toBeDefined());
    expect(screen.queryByText('分区1（XFS）')).toBeNull();

    fireEvent.click(windowsTreeNode);
    fireEvent.click(linuxTreeNode);

    await waitFor(() => expect(screen.getByText('分区1（XFS）')).toBeDefined());
    expect(screen.queryByText('分区1（NTFS）')).toBeNull();
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
        true,
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

describe('FileBrowser render', () => {
  it('renders without crashing when a case is active', () => {
    const { container } = renderPage();
    expect(container).toBeTruthy();
  });
});
