import { ArrowUp } from 'lucide-react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { FileIconWithStatusOverlay } from '@/features/files/components/FileIconWithStatusOverlay';
import type { FileEntryRow } from '@/types/models';

// Module-level columns: a stable reference keeps DenseDataTable's memoized
// rows from re-rendering on every parent render.
const FILE_LIST_COLUMNS: DenseColumn<FileEntryRow>[] = [
  {
    key: 'name',
    title: '名称',
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
    title: '大小',
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
    title: '修改时间',
    className: 'w-44 text-forensics-muted',
    sortable: true,
    sortKey: 'modifiedAt',
    render: (row) => row.modifiedAt ?? '-',
  },
  {
    key: 'attr',
    title: '属性',
    className: 'text-forensics-muted-light',
    render: (row) =>
      row.entryType === 'directory'
        ? 'DIR'
        : 'A--',
  },
];

export interface FileListPanelProps {
  sortedRows: FileEntryRow[];
  selectedFileId: string | undefined;
  viewerTab?: 'metadata' | 'text' | 'hex' | 'preview';
  fileSortKey: string;
  fileSortDirection: 'asc' | 'desc';
  handleSort: (key: string) => void;
  setSelectedDirectoryId: (id: string) => void;
  setSelectedFileId: (id: string | undefined) => void;
  setExpandedDirectoryIds: (updater: (prev: string[]) => string[]) => void;
  parentDirectory?: { id: string; name: string };
  goToParentDirectory?: () => void;
  rowsPage: { offset: number; limit: number; totalCount: number; rows: FileEntryRow[]; truncated: boolean } | undefined;
  canGoToPreviousRows: boolean;
  canGoToNextRows: boolean;
  goToPreviousRows: () => void;
  goToNextRows: () => void;
}

export function FileListPanel({
  sortedRows,
  selectedFileId,
  viewerTab,
  fileSortKey,
  fileSortDirection,
  handleSort,
  setSelectedDirectoryId,
  setSelectedFileId,
  setExpandedDirectoryIds,
  parentDirectory,
  goToParentDirectory,
  rowsPage,
  canGoToPreviousRows,
  canGoToNextRows,
  goToPreviousRows,
  goToNextRows,
}: FileListPanelProps) {
  const { t } = useTranslation();
  const selectedFile = sortedRows.find((row) => row.id === selectedFileId);
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

  return (
    <div className="flex-1 flex flex-col border-b border-forensics-border bg-forensics-surface min-h-0">
      <div className="shrink-0 flex items-center gap-2 border-b border-forensics-border bg-forensics-panel px-3 py-1.5">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 gap-1 px-2 text-[11px]"
          disabled={!parentDirectory}
          onClick={goToParentDirectory}
        >
          <ArrowUp size={12} />
          {t('fileBrowser.parentDirectory')}
        </Button>
      </div>
      <DenseDataTable<FileEntryRow>
        rows={sortedRows}
        getRowKey={(row) => row.id}
        selectedRowKey={selectedFile?.id}
        onRowClick={handleRowClick}
        emptyTitle="当前目录为空"
        emptyDescription={
          rowsPage?.truncated
            ? `当前仅加载前 ${rowsPage.limit} 项，共 ${rowsPage.totalCount} 项。`
            : '所选路径下没有可展示的文件对象。'
        }
        sortKey={fileSortKey}
        sortDirection={fileSortDirection}
        onSort={handleSort}
        columns={FILE_LIST_COLUMNS}
      />
      {rowsPage && viewerTab !== 'hex' ? (
        <div className="flex items-center justify-between border-t border-forensics-border bg-forensics-panel px-3 py-2 text-[11px] text-forensics-muted">
          <span>
            显示第 {rowsPage.totalCount === 0 ? 0 : rowsPage.offset + 1} - {Math.min(rowsPage.offset + rowsPage.rows.length, rowsPage.totalCount)} 项，共 {rowsPage.totalCount} 项
          </span>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 px-2 text-[11px]"
              onClick={goToPreviousRows}
              disabled={!canGoToPreviousRows}
            >
              上一页
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 px-2 text-[11px]"
              onClick={goToNextRows}
              disabled={!canGoToNextRows}
            >
              下一页
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
