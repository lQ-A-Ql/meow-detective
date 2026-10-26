import { readFileSync } from 'node:fs';
import { useState } from 'react';
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { QueryCache, QueryClient, QueryClientProvider, type QueryKey } from '@tanstack/react-query';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { useSelectionStore } from '@/stores/selection-store';
import { useFileNavigationStore } from '@/stores/file-navigation-store';
import { useUiStore } from '@/stores/ui-store';
import { useFileTree } from '@/features/files/hooks/use-file-tree';
import { useFileBrowserLocation } from '@/features/files/hooks/use-file-browser-location';
import { useFilePagination } from '@/features/files/hooks/use-file-pagination';
import { useFileBrowserCoordinator } from '@/features/files/hooks/use-file-browser-coordinator';
import { resetFileTreeExpansion } from '@/features/files/file-tree-expansion-store';
import { FileTreePanel } from '@/features/files/components/FileTreePanel';
import { FileListPanel } from '@/features/files/components/FileListPanel';
import { FileVisibilityToggle } from '@/features/files/components/FileVisibilityToggle';
import { DENSE_TABLE_ROW_HEIGHT } from '@/components/tables/dense-table-metrics';
import type { DataSourceSummary, FileJumpContext, FileRowsPage } from '@/types/models';

interface ServiceSnapshot {
  caseId: string;
  sources: DataSourceSummary[];
  queries: { key: QueryKey; data: unknown }[];
  rootId: string;
  directoryId: string;
  offPageId: string;
  listingId: string;
  fileId: string;
  farFileId: string;
  hiddenId: string;
  hiddenParentId: string;
}

const fixturePath = process.env.FORENSICS_FILE_BROWSER_RESPONSES;
const responses: ServiceSnapshot[] = fixturePath ? JSON.parse(readFileSync(fixturePath, 'utf8')) : [];
const errors: unknown[] = [];
let client: QueryClient;

function Browser({ source }: { source: ServiceSnapshot }) {
  const selectedDirectoryId = useSelectionStore((state) => state.selectedDirectoryId);
  const selectedFileId = useSelectionStore((state) => state.selectedFileId);
  const [showHidden, onShowHidden] = useState(true);
  const location = useFileBrowserLocation({ caseId: source.caseId, showHidden, pageLimit: 500,
    sortKey: 'name', sortDirection: 'asc' });
  const tree = useFileTree({ caseId: source.caseId, showHidden, pageLimit: 500, selectedDirectoryId,
    setSelectedDirectoryId: location.selectDirectory, setSelectedFileId: location.selectFile,
    partitions: source.sources.flatMap((item) => item.partitions ?? []), dataSources: source.sources,
    revealDirectoryIds: location.revealDirectoryIds, jumpPathNodes: location.pathNodes,
    deferInitialSelection: location.deferInitialSelection });
  const pagination = useFilePagination({ caseId: source.caseId, showHidden, pageLimit: 500,
    activeDirectoryId: tree.activeDirectoryIsDataSource ? undefined : tree.activeDirectoryId });
  const navigation = useFileBrowserCoordinator({ caseId: source.caseId, selectedDirectoryId, selectedFileId,
    showHidden, onShowHidden, location, tree, pagination });
  return <>
    <output data-testid="location">{JSON.stringify({ selectedDirectoryId, selectedFileId,
      offset: pagination.rowsOffset, treeIds: tree.treeNodes.map((node) => node.id), showHidden })}</output>
    <div style={{ height: 600 }}><FileTreePanel filteredTreeNodes={tree.filteredTreeNodes}
      activeDirectoryId={tree.activeDirectoryId} expandedIds={tree.expandedIdSet}
      activeChildrenPage={tree.activeChildrenPage} activeTreeChildrenLoaded={tree.activeTreeChildrenLoaded}
      canLoadMoreTreeChildren={tree.canLoadMoreTreeChildren} loadMoreActiveTreeChildren={tree.loadMoreActiveTreeChildren}
      toggleDirectory={tree.toggleDirectory} displayNodeName={tree.displayNodeName} filterQuery={tree.filterQuery}
      setFilterQuery={tree.setFilterQuery} treeWidth={tree.treeWidth} isResizing={tree.isResizing}
      onResizeStart={tree.onResizeStart} treeContainerRef={tree.treeContainerRef}
      dataSources={source.sources} FILE_BROWSER_PAGE_LIMIT={500} /></div>
    <div data-testid="listing" style={{ height: 600 }}><FileListPanel sortedRows={pagination.sortedRows}
      selectedFileId={selectedFileId} fileSortKey="name" fileSortDirection="asc" handleSort={navigation.handleSort}
      onActivateEntry={navigation.activateEntry} loadContextKey={pagination.rowsContextKey}
      parentDirectory={tree.parentDirectory} goToParentDirectory={tree.goToParentDirectory}
      rowsPage={pagination.rowsPage} canGoToPreviousRows={pagination.canGoToPreviousRows}
      canGoToNextRows={pagination.canGoToNextRows} goToPreviousRows={navigation.goToPreviousRows}
      goToNextRows={navigation.goToNextRows} visibilityToggle={<FileVisibilityToggle checked={showHidden} onCheckedChange={navigation.setShowHidden} />}
      onExtractFile={() => { throw new Error('Extraction is outside this acceptance'); }} /></div>
  </>;
}

function location() {
  return JSON.parse(screen.getByTestId('location').textContent!) as {
    selectedDirectoryId?: string; selectedFileId?: string; offset: number; treeIds: string[]; showHidden: boolean;
  };
}

function open(source: ServiceSnapshot, directory = source.listingId, fileId?: string) {
  errors.length = 0;
  resetFileTreeExpansion();
  useFileNavigationStore.getState().cancel();
  useSelectionStore.setState({ selectedDirectoryId: directory, selectedFileId: fileId });
  useUiStore.setState({ fileSortKey: 'name', fileSortDirection: 'asc' });
  client = new QueryClient({ queryCache: new QueryCache({ onError: (error, query) => errors.push({ error, key: query.queryKey }) }),
    defaultOptions: { queries: { staleTime: Infinity, retry: false, refetchOnMount: false, refetchOnWindowFocus: false, gcTime: Infinity } } });
  for (const query of source.queries) client.setQueryData(query.key, query.data);
  render(<QueryClientProvider client={client}><Browser source={source} /></QueryClientProvider>);
}

function rows(source: ServiceSnapshot, id: string, offset = 0) {
  return source.queries.find((query) => query.key[1] === 'rows-page' && query.key[3] === id
    && query.key[4] === offset && query.key[6] === true)!.data as FileRowsPage;
}

async function clickEntry(source: ServiceSnapshot, parent: string, id: string, offset = 0) {
  const page = rows(source, parent, offset);
  const index = page.rows.findIndex((row) => row.id === id);
  expect(index).toBeGreaterThanOrEqual(0);
  const listing = screen.getByTestId('listing');
  const viewport = listing.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]')!;
  viewport.scrollTop = index * DENSE_TABLE_ROW_HEIGHT;
  fireEvent.scroll(viewport);
  fireEvent.click(await within(listing).findByText(page.rows[index].name));
}

afterEach(() => {
  cleanup();
  client?.clear();
  expect(errors).toEqual([]);
  useFileNavigationStore.getState().cancel();
});

describe.skipIf(!fixturePath)('file browser with real source-service responses', () => {
  beforeAll(() => expect(responses).toHaveLength(2));
  for (let index = 0; index < 2; index += 1) {
    it(`keeps manual paging after file selection in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source);
      await clickEntry(source, source.listingId, source.fileId);
      expect(location().selectedFileId).toBe(source.fileId);
      fireEvent.click(within(screen.getByTestId('listing')).getByRole('button', { name: '下一页' }));
      await waitFor(() => expect(location().offset).toBe(500));
      expect(location().selectedFileId).toBeUndefined();
      expect(useFileNavigationStore.getState().pending).toBeUndefined();
    });

    it(`ends restored file navigation before subsequent paging in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source, source.listingId, source.fileId);
      await waitFor(() => expect(useFileNavigationStore.getState().request).toBeUndefined());
      fireEvent.click(within(screen.getByTestId('listing')).getByRole('button', { name: '下一页' }));
      await waitFor(() => expect(location().offset).toBe(500));
    });

    it(`keeps hidden entries disabled after selecting a hidden file in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source, source.hiddenParentId);
      await clickEntry(source, source.hiddenParentId, source.hiddenId);
      fireEvent.click(screen.getByTestId('file-visibility-toggle'));
      await waitFor(() => expect(location().showHidden).toBe(false));
      expect(location().selectedFileId).toBeUndefined();
      expect(screen.getByTestId('file-visibility-toggle')).toHaveAttribute('aria-pressed', 'false');
    });

    it(`retains off-page directories and sibling partitions in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source, source.directoryId);
      fireEvent.click(within(screen.getByTestId('listing')).getByRole('button', { name: '下一页' }));
      await waitFor(() => expect(location().offset).toBe(500));
      await clickEntry(source, source.directoryId, source.offPageId, 500);
      await waitFor(() => expect(location().treeIds).toContain(source.offPageId));
      expect(location().selectedDirectoryId).toBe(source.offPageId);
      const roots = source.queries.find((query) => query.key[1] === 'tree' && query.key[3] === true)!.data as { id: string }[];
      expect(location().treeIds).toEqual(expect.arrayContaining(roots.map((node) => node.id)));
    });

    it(`opens a list directory while its tree parent is collapsed in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source, source.rootId);
      const child = rows(source, source.rootId).rows.find((row) => row.name === 'usr')!;
      await waitFor(() => expect(document.getElementById(source.rootId)).toHaveAttribute('aria-expanded', 'false'));
      await clickEntry(source, source.rootId, child.id);
      await waitFor(() => expect(location().treeIds).toContain(child.id));
      expect(location().selectedDirectoryId).toBe(child.id);
      expect(document.getElementById(source.rootId)).toHaveAttribute('aria-expanded', 'true');
    });

    it(`consumes external file navigation before manual paging in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source, source.rootId);
      const context = source.queries.find((query) => query.key[1] === 'jump-context'
        && query.key[3] === source.fileId && query.key[4] === true)!.data as FileJumpContext;
      act(() => {
        const store = useFileNavigationStore.getState();
        const request = store.begin(source.caseId, source.fileId, 'external');
        expect(store.resolve(request, context)).toBe(true);
      });
      await waitFor(() => expect(useFileNavigationStore.getState().pending).toBeUndefined());
      expect(location().selectedDirectoryId).toBe(source.listingId);
      fireEvent.click(within(screen.getByTestId('listing')).getByRole('button', { name: '下一页' }));
      await waitFor(() => expect(location().offset).toBe(500));
    });

    it(`keeps an off-page external target selected until its page is applied in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source, source.rootId);
      const context = source.queries.find((query) => query.key[1] === 'jump-context'
        && query.key[3] === source.farFileId && query.key[4] === true)!.data as FileJumpContext;
      expect(context.rowOffset).toBe(500);
      act(() => {
        const store = useFileNavigationStore.getState();
        expect(store.resolve(store.begin(source.caseId, source.farFileId, 'external'), context)).toBe(true);
      });
      await waitFor(() => expect(useFileNavigationStore.getState().pending).toBeUndefined());
      expect(location()).toMatchObject({ selectedDirectoryId: source.listingId,
        selectedFileId: source.farFileId, offset: 500 });
      expect(await within(screen.getByTestId('listing')).findByText(context.target.name)).toBeVisible();
    });

    it(`starts at the first page after leaving and returning to a directory in sample ${index + 1}`, () => {
      const source = responses[index];
      errors.length = 0;
      useUiStore.setState({ fileSortKey: 'name', fileSortDirection: 'asc' });
      client = new QueryClient({ queryCache: new QueryCache({ onError: (error) => errors.push(error) }),
        defaultOptions: { queries: { staleTime: Infinity, retry: false } } });
      for (const query of source.queries) client.setQueryData(query.key, query.data);
      const { result, rerender } = renderHook(({ directory }) => useFilePagination({
        caseId: source.caseId, activeDirectoryId: directory, pageLimit: 500, showHidden: true,
      }), { initialProps: { directory: source.listingId }, wrapper: ({ children }) =>
        <QueryClientProvider client={client}>{children}</QueryClientProvider> });
      act(() => result.current.setRowsOffset(500));
      expect(result.current.rowsOffset).toBe(500);
      rerender({ directory: source.rootId });
      expect(result.current.rowsOffset).toBe(0);
      rerender({ directory: source.listingId });
      expect(result.current.rowsOffset).toBe(0);
    });

    it(`handles focused tree keys once and uses the current expansion in sample ${index + 1}`, async () => {
      const source = responses[index];
      open(source);
      const root = await waitFor(() => {
        const element = document.getElementById(source.rootId);
        expect(element).not.toBeNull();
        return element!;
      });
      await waitFor(() => expect(root).toHaveAttribute('aria-expanded', 'true'));
      act(() => root.focus());
      fireEvent.keyDown(root, { key: 'Enter' });
      await waitFor(() => expect(root).toHaveAttribute('aria-expanded', 'false'));
      fireEvent.keyDown(root, { key: 'ArrowRight' });
      await waitFor(() => expect(root).toHaveAttribute('aria-expanded', 'true'));
      fireEvent.keyDown(root, { key: 'ArrowLeft' });
      await waitFor(() => expect(root).toHaveAttribute('aria-expanded', 'false'));
    });
  }

  it('discards a completed lookup after a manual navigation cancelled it', () => {
    errors.length = 0;
    const source = responses[0];
    const store = useFileNavigationStore.getState();
    const request = store.begin(source.caseId, source.fileId, 'external');
    store.cancel();
    useSelectionStore.setState({ selectedDirectoryId: source.directoryId, selectedFileId: undefined });
    const context = source.queries.find((query) => query.key[1] === 'jump-context' && query.key[3] === source.fileId)!.data as FileJumpContext;
    expect(store.resolve(request, context)).toBe(false);
    expect(useSelectionStore.getState().selectedDirectoryId).toBe(source.directoryId);
  });

  it('rejects an older case request after a newer case owns navigation', () => {
    errors.length = 0;
    const store = useFileNavigationStore.getState();
    const oldRequest = store.begin(responses[0].caseId, responses[0].fileId, 'external');
    const newRequest = store.begin(responses[1].caseId, responses[1].fileId, 'external');
    const contextFor = (source: ServiceSnapshot) => source.queries.find((query) => query.key[1] === 'jump-context'
      && query.key[3] === source.fileId && query.key[4] === true)!.data as FileJumpContext;
    expect(store.resolve(oldRequest, contextFor(responses[0]))).toBe(false);
    expect(store.resolve(newRequest, contextFor(responses[1]))).toBe(true);
    expect(useSelectionStore.getState().selectedFileId).toBe(responses[1].fileId);
  });
});
