import { ChevronLeft, ChevronRight } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import type { ReactNode } from 'react';
import { useCallback, useMemo, type ReactElement } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { FileIconWithStatusOverlay } from '@/features/files/components/FileIconWithStatusOverlay';
import { FileEntryContextMenu } from '@/features/files/components/FileEntryContextMenu';
import type { FileEntryRow } from '@/types/models';

/**
 * `ls -l` style ten-character rendering of a Unix mode: one type character
 * (d/l/-/p/c/b/s) followed by nine rwx permission characters.
 */
export function formatUnixMode(mode: number): string {
  const typeChars: Record<number, string> = {
    0o01: 'p',
    0o02: 'c',
    0o04: 'd',
    0o06: 'b',
    0o10: '-',
    0o12: 'l',
    0o14: 's',
  };
  const typeChar = typeChars[(mode & 0o170000) >> 12] ?? '?';
  let permissions = '';
  for (let bit = 8; bit >= 0; bit -= 1) {
    permissions += mode & (1 << bit) ? 'rwx'[(8 - bit) % 3] : '-';
  }
  return typeChar + permissions;
}

/**
 * Compact attribute letters from real entry flags:
 * directories render as "D"; files concatenate set bits in R/H/S/A order
 * (readOnly, hidden, system, archive) and render "-" when none are set.
 * Entries from Unix filesystems (unixMode present) render in `ls -l` form.
 */
export function formatEntryAttributes(row: FileEntryRow): string {
  if (row.unixMode !== undefined && row.unixMode !== null) {
    return formatUnixMode(row.unixMode);
  }
  if (row.entryType === 'directory') {
    return 'D';
  }
  let flags = '';
  if (row.readOnly) flags += 'R';
  if (row.hidden) flags += 'H';
  if (row.system) flags += 'S';
  if (row.archive) flags += 'A';
  return flags || '-';
}

export interface FileListPanelProps {
  sortedRows: FileEntryRow[];
  selectedFileId: string | undefined;
  fileSortKey: string;
  fileSortDirection: 'asc' | 'desc';
  handleSort: (key: string) => void;
  setSelectedDirectoryId: (id: string) => void;
  setSelectedFileId: (id: string | undefined) => void;
  setExpandedDirectoryIds: (updater: (prev: string[]) => string[]) => void;
  parentDirectory?: { id: string; name: string };
  goToParentDirectory?: () => void;
  /** Controls rendered in the list toolbar, next to the parent-directory button. */
  visibilityToggle?: ReactNode;
  mountControl?: ReactNode;
  rowsPage: { offset: number; limit: number; totalCount: number; rows: FileEntryRow[]; truncated: boolean } | undefined;
  canGoToPreviousRows: boolean;
  canGoToNextRows: boolean;
  goToPreviousRows: () => void;
  goToNextRows: () => void;
  onExtractFile: (file: FileEntryRow) => void;
}

export function FileListPanel({
  sortedRows,
  selectedFileId,
  fileSortKey,
  fileSortDirection,
  handleSort,
  setSelectedDirectoryId,
  setSelectedFileId,
  setExpandedDirectoryIds,
  parentDirectory,
  goToParentDirectory,
  visibilityToggle,
  mountControl,
  rowsPage,
  canGoToPreviousRows,
  canGoToNextRows,
  goToPreviousRows,
  goToNextRows,
  onExtractFile,
}: FileListPanelProps) {
  const { t } = useTranslation();
  const selectedFile = sortedRows.find((row) => row.id === selectedFileId);
  // Column labels are translated, so the array is rebuilt only when the active
  // language changes; a stable reference keeps DenseDataTable's memoized rows
  // from re-rendering on every parent render.
  const columns = useMemo<DenseColumn<FileEntryRow>[]>(
    () => [
      {
        key: 'name',
        title: t('common.sort.name'),
        className: 'w-[34%]',
        sortable: true,
        sortKey: 'name',
        render: (row) => (
          <div className="flex items-center gap-2 min-w-0">
            <FileIconWithStatusOverlay
              name={row.name}
              entryType={row.entryType}
              deleted={row.deleted}
              hidden={row.hidden}
              system={row.system}
              size={12}
            />
            <span className="truncate">{row.name}</span>
          </div>
        ),
      },
      {
        key: 'size',
        title: t('common.sort.size'),
        className: 'w-28 text-forensics-muted',
        sortable: true,
        sortKey: 'size',
        render: (row) =>
          row.entryType === 'directory'
            ? '-'
            : row.size
              ? `${Math.round(row.size / 1000)} KB`
              : '0 KB',
      },
      {
        key: 'modifiedAt',
        title: t('common.sort.modifiedAt'),
        className: 'w-44 text-forensics-muted',
        sortable: true,
        sortKey: 'modifiedAt',
        render: (row) => row.modifiedAt ?? '-',
      },
      {
        key: 'attr',
        title: t('fileBrowser.list.columnAttributes'),
        className: 'text-forensics-muted-light',
        render: (row) => formatEntryAttributes(row),
      },
    ],
    [t],
  );
  const handleRowClick = useCallback(
    (row: FileEntryRow) => {
      if (row.entryType === 'directory') {
        setSelectedDirectoryId(row.id);
        setSelectedFileId(undefined);
        setExpandedDirectoryIds((current) =>
          current.includes(row.id)
            ? current
            : [...current, row.id]
        );
        return;
      }
      setSelectedFileId(row.id);
    },
    [setSelectedDirectoryId, setSelectedFileId, setExpandedDirectoryIds],
  );
  const renderRowContextMenu = useCallback(
    (row: FileEntryRow, trigger: ReactElement) => (
      <FileEntryContextMenu
        file={row}
        onExtractFile={onExtractFile}
      >
        {trigger}
      </FileEntryContextMenu>
    ),
    [onExtractFile, setSelectedFileId],
  );

  return (
    <div className="flex-1 flex flex-col border-b border-forensics-divider bg-forensics-surface min-h-0">
      <div className="shrink-0 flex items-center gap-2 border-b border-forensics-divider bg-forensics-panel px-3 py-1.5">
        <Tooltip content={t('fileBrowser.parentDirectory')}><Button aria-label={t('fileBrowser.parentDirectory')}
          type="button"
          variant="outline"
          size="sm"
          className="text-[11px]"
          disabled={!parentDirectory}
          onClick={goToParentDirectory}
        ><ActionIcon name="arrow-up" /></Button></Tooltip>
        {visibilityToggle}
        {mountControl}
        {canGoToPreviousRows || canGoToNextRows ? (
          <div className="ml-auto flex items-center gap-1">
            <Button
              type="button"
              variant="viewerControl"
              size="iconXs"
              onClick={goToPreviousRows}
              disabled={!canGoToPreviousRows}
              aria-label={t('fileBrowser.list.previousPage')}
              title={t('fileBrowser.list.previousPage')}
            >
              <ChevronLeft size={12} />
            </Button>
            <Button
              type="button"
              variant="viewerControl"
              size="iconXs"
              onClick={goToNextRows}
              disabled={!canGoToNextRows}
              aria-label={t('fileBrowser.list.nextPage')}
              title={t('fileBrowser.list.nextPage')}
            >
              <ChevronRight size={12} />
            </Button>
          </div>
        ) : null}
      </div>
      <DenseDataTableFrame layout="fill" variant="plain">
        <DenseDataTable<FileEntryRow>
        rows={sortedRows}
        getRowKey={(row) => row.id}
        selectedRowKey={selectedFile?.id}
        onRowClick={handleRowClick}
        renderRowContextMenu={renderRowContextMenu}
        emptyTitle={t('fileBrowser.list.emptyTitle')}
        emptyAction={parentDirectory ? (
          <Tooltip content={t('fileBrowser.parentDirectory')}><Button aria-label={t('fileBrowser.parentDirectory')} type="button" variant="outline" size="sm" onClick={goToParentDirectory}><ActionIcon name="arrow-up" /></Button></Tooltip>
        ) : undefined}
        emptyDescription={
          rowsPage?.truncated
            ? t('fileBrowser.list.emptyTruncated', { limit: rowsPage.limit, total: rowsPage.totalCount })
            : t('fileBrowser.list.emptyDescription')
        }
        sortKey={fileSortKey}
        sortDirection={fileSortDirection}
        onSort={handleSort}
        columns={columns}
        />
      </DenseDataTableFrame>
    </div>
  );
}
