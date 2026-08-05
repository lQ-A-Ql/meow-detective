import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FileEntryRow } from '@/types/models';
import { FileListPanel } from './FileListPanel';

const FILE: FileEntryRow = {
  id: 'file-1',
  name: 'evidence.bin',
  path: '/evidence.bin',
  entryType: 'file',
  size: 128,
  deleted: false,
  hidden: false,
  system: false,
};

const DIRECTORY: FileEntryRow = {
  ...FILE,
  id: 'directory-1',
  name: 'Documents',
  path: '/Documents',
  entryType: 'directory',
  size: undefined,
};

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe = vi.fn();
    disconnect = vi.fn();
    unobserve = vi.fn();
  });
});

function renderPanel(
  rows: FileEntryRow[],
  onExtractFile = vi.fn(),
  pagination: { canGoToPreviousRows?: boolean; canGoToNextRows?: boolean } = {},
) {
  const setSelectedFileId = vi.fn();
  const goToPreviousRows = vi.fn();
  const goToNextRows = vi.fn();
  render(
    <FileListPanel
      sortedRows={rows}
      selectedFileId={undefined}
      fileSortKey="name"
      fileSortDirection="asc"
      handleSort={vi.fn()}
      setSelectedDirectoryId={vi.fn()}
      setSelectedFileId={setSelectedFileId}
      setExpandedDirectoryIds={vi.fn()}
      rowsPage={{ offset: 0, limit: 500, totalCount: rows.length, rows, truncated: false }}
      canGoToPreviousRows={pagination.canGoToPreviousRows ?? false}
      canGoToNextRows={pagination.canGoToNextRows ?? false}
      goToPreviousRows={goToPreviousRows}
      goToNextRows={goToNextRows}
      onExtractFile={onExtractFile}
    />,
  );
  return { goToNextRows, goToPreviousRows, onExtractFile, setSelectedFileId };
}

describe('FileListPanel extraction context menu', () => {
  it('does not render the redundant paging footer', () => {
    renderPanel([FILE]);

    expect(screen.queryByText(/显示第/)).toBeNull();
    expect(screen.queryByRole('button', { name: '上一页' })).toBeNull();
    expect(screen.queryByRole('button', { name: '下一页' })).toBeNull();
  });

  it('keeps compact paging controls when another file page exists', () => {
    const { goToNextRows } = renderPanel([FILE], vi.fn(), { canGoToNextRows: true });

    expect(screen.queryByText(/显示第/)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '下一页' }));
    expect(goToNextRows).toHaveBeenCalledOnce();
  });

  it('opens extraction without selecting the right-clicked file', async () => {
    const { onExtractFile, setSelectedFileId } = renderPanel([FILE]);

    fireEvent.contextMenu(screen.getByText('evidence.bin'));
    const extractItem = await screen.findByText('提取文件');
    expect(setSelectedFileId).not.toHaveBeenCalled();

    fireEvent.click(extractItem);
    expect(onExtractFile).toHaveBeenCalledWith(FILE);
  });

  it('does not expose file extraction for directory rows', () => {
    renderPanel([DIRECTORY]);

    fireEvent.contextMenu(screen.getByText('Documents'));
    expect(screen.queryByText('提取文件')).toBeNull();
  });
});
