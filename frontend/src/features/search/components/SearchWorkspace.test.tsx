import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SearchWorkspace } from './SearchWorkspace';
import type { SearchWorkspaceModel } from '@/features/search/use-search-workspace-model';
import type { SearchFileHit } from '@/types/search';

vi.mock('./SearchFilePreviewDialog', () => ({
  SearchFilePreviewDialog: ({ model }: { model: { open: boolean } }) => (
    model.open ? <div>搜索文件预览</div> : null
  ),
}));

const hit: SearchFileHit = {
  fileId: 'file-1',
  dataSourceId: 'source-1',
  dataSourceName: '检材2.E01',
  name: 'report.7z',
  path: 'Users/alice/Downloads/report.7z',
  entryType: 'file',
  extension: '7z',
  size: 3 * 1024 * 1024,
  modifiedAt: '2026-07-29T12:00:00Z',
  deleted: false,
  hidden: false,
  system: false,
  encrypted: true,
};

function createModel(overrides: Partial<SearchWorkspaceModel> = {}) {
  const model = {
    activeQuery: 'report',
    clearQuery: vi.fn(),
    coverage: {
      readySourceCount: 2,
      indexedSourceCount: 1,
      expectedEntryCount: 100,
      indexedEntryCount: 80,
      missingSourceIds: ['source-2'],
      complete: false,
    },
    dataSources: [{ id: 'source-1', name: '检材2.E01' }],
    extensionInput: '',
    hasMore: false,
    initialLoadFailed: false,
    loadContextKey: 'report',
    loadMoreFailed: false,
    loadNextPage: vi.fn(),
    loadingMore: false,
    onHitRowClick: vi.fn(),
    options: {
      matchPath: false,
      entryType: 'any' as const,
      extensions: [],
      dataSourceIds: [],
      sortKey: 'name' as const,
      sortDirection: 'asc' as const,
    },
    queryInput: 'report',
    preview: { open: false },
    retry: vi.fn(),
    searchHits: [hit],
    searchQueryStateKey: 1,
    searchTookMs: 4,
    selectedHit: hit,
    setOption: vi.fn(),
    setExtensionInput: vi.fn(),
    setQueryInput: vi.fn(),
    sortDirection: 'asc' as const,
    sortKey: 'name' as const,
    toggleSort: vi.fn(),
    totalHits: 1,
    truncated: false,
    ...overrides,
  } as SearchWorkspaceModel;
  return model;
}

describe('SearchWorkspace', () => {
  it('renders file metadata and incomplete index coverage', () => {
    render(<SearchWorkspace model={createModel()} />);

    expect(screen.getByRole('textbox', { name: '文件名搜索' })).toHaveValue('report');
    expect(screen.getByText('report.7z')).toBeInTheDocument();
    expect(screen.getByText('Users/alice/Downloads/report.7z')).toBeInTheDocument();
    expect(screen.getByText(/索引覆盖不完整，需要重新分析以重建索引 80\/100/)).toBeInTheDocument();
  });

  it('opens the preview from a single result-row click', () => {
    const model = createModel();
    render(<SearchWorkspace model={model} />);

    const row = screen.getByText('report.7z').closest('tr');
    expect(row).not.toBeNull();
    fireEvent.click(row!);
    expect(model.onHitRowClick).toHaveBeenCalledWith(hit);
  });

  it('passes sortable column clicks to the workspace model', () => {
    const model = createModel();
    render(<SearchWorkspace model={model} />);

    fireEvent.click(screen.getByText('修改时间'));
    expect(model.toggleSort).toHaveBeenCalledWith('modifiedAt');
  });

  it('normalizes extension filter input before updating the model', () => {
    const model = createModel();
    render(<SearchWorkspace model={model} />);

    fireEvent.change(screen.getByRole('textbox', { name: '扩展名筛选' }), {
      target: { value: '.txt; log,7z' },
    });
    expect(model.setExtensionInput).toHaveBeenCalledWith('.txt; log,7z');
  });
});
