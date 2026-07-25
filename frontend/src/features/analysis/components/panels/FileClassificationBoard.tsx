import { useCallback, useMemo, useState } from 'react';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { CollapsibleSection } from '@/components/layout/CollapsibleSection';
import { FilePreviewDialog } from '@/components/preview/FilePreviewDialog';
import type { FilePreviewKind, PreviewViewerTab } from '@/components/preview/FilePreviewTabs';
import {
  useDocumentPreview,
  useFileHandle,
  useFileViewer,
  useImagePreview,
  useMediaUrl,
  useTextPreview,
} from '@/features/files/hooks';
import type {
  ApiErrorDto,
  ClassifiedFileRow,
  ClassificationSubcategory,
  FileClassificationBoard as FileClassificationBoardData,
  FileEntryRow,
} from '@/types/models';
import {
  CATEGORY_COLORS,
  CATEGORY_ICONS,
  DenseTableFrame,
  EmptyLine,
  formatSize,
  StatusPill,
  SummaryStrip,
  WarningList,
} from './helpers';

const FAMILY_KEY: Record<string, string> = {
  documents: 'Documents',
  images: 'Images',
  media: 'Media',
  databases: 'Databases',
  executables: 'Executables',
  archives: 'Archives',
  system: 'System',
  forensics: 'Forensics',
  other: 'Other',
};

const TEXT_EXTENSIONS = new Set(['log', 'txt', 'md', 'csv', 'json', 'xml', 'html', 'htm', 'evtx']);

// Module-level columns keep a stable reference so DenseDataTable's memoized
// rows do not re-render whenever dialog/viewer state changes above them.
const CLASSIFICATION_COLUMNS: DenseColumn<ClassifiedFileRow>[] = [
  {
    key: 'name',
    title: '文件名',
    className: 'min-w-[220px]',
    render: (row) => row.name,
  },
  {
    key: 'path',
    title: '路径',
    className: 'min-w-[260px]',
    render: (row) => <span className="font-mono text-[10px] text-forensics-muted-light">{row.path}</span>,
  },
  {
    key: 'magicType',
    title: '魔数类型',
    className: 'w-[90px]',
    render: (row) => row.magicType ?? '-',
  },
  {
    key: 'source',
    title: '判定',
    className: 'w-[70px]',
    render: (row) => (
      <span className={row.classificationSource === 'magic' ? 'text-forensics-success-text' : 'text-forensics-muted'}>
        {row.classificationSource === 'magic' ? '魔数' : '推断'}
      </span>
    ),
  },
  {
    key: 'size',
    title: '大小',
    className: 'w-[100px] text-right',
    render: (row) => formatSize(row.size),
  },
];

function defaultTabFor(row: ClassifiedFileRow): PreviewViewerTab {
  const ext = row.name.split('.').pop()?.toLowerCase() ?? '';
  if (row.magicType && ['JPEG', 'PNG', 'GIF', 'BMP', 'WEBP', 'ICO'].includes(row.magicType)) {
    return 'preview';
  }
  if (
    ['pdf', 'docx', 'xlsx', 'pptx', 'sqlite', 'sqlite3', 'db', 'db3', 'doc', 'xls', 'ppt'].includes(
      ext,
    )
  ) {
    return 'preview';
  }
  if (['mp4', 'webm', 'avi', 'mkv', 'mp3', 'wav', 'flac', 'aac', 'ogg'].includes(ext)) {
    return 'preview';
  }
  if (TEXT_EXTENSIONS.has(ext)) {
    return 'text';
  }
  return 'hex';
}

function toFileEntryRow(row: ClassifiedFileRow): FileEntryRow {
  const ext = row.name.includes('.') ? row.name.split('.').pop() : undefined;
  return {
    id: row.fileId,
    path: row.path,
    name: row.name,
    entryType: 'file',
    size: row.size,
    ext,
    deleted: false,
    hidden: false,
    system: false,
  };
}

function previewKindFor(row: ClassifiedFileRow): FilePreviewKind | undefined {
  const ext = row.name.split('.').pop()?.toLowerCase() ?? '';
  if (['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'ico'].includes(ext)) return 'image';
  if (['mp4', 'webm', 'avi', 'mkv'].includes(ext)) return 'video';
  if (['mp3', 'wav', 'flac', 'aac', 'ogg'].includes(ext)) return 'audio';
  if (
    ['pdf', 'docx', 'xlsx', 'pptx', 'sqlite', 'sqlite3', 'db', 'db3', 'doc', 'xls', 'ppt'].includes(
      ext,
    )
  ) {
    return 'document';
  }
  return undefined;
}

export function FileClassificationBoard({ board }: { board?: FileClassificationBoardData }) {
  const [selected, setSelected] = useState<ClassifiedFileRow | null>(null);
  const [viewerTab, setViewerTab] = useState<PreviewViewerTab>('hex');
  const selectedFile = useMemo(
    () => (selected ? toFileEntryRow(selected) : undefined),
    [selected],
  );
  const previewKind = useMemo(
    () => (selected ? previewKindFor(selected) : undefined),
    [selected],
  );
  const hexEnabled = viewerTab === 'hex' && Boolean(selectedFile?.id);
  const textEnabled = viewerTab === 'text' && Boolean(selectedFile?.id);
  const imageEnabled = viewerTab === 'preview' && previewKind === 'image' && Boolean(selectedFile?.id);
  const mediaEnabled = viewerTab === 'preview'
    && (previewKind === 'video' || previewKind === 'audio') && Boolean(selectedFile?.id);
  const documentEnabled = viewerTab === 'preview' && previewKind === 'document' && Boolean(selectedFile?.id);
  const needsHandle = viewerTab === 'metadata' || (viewerTab === 'preview' && previewKind === undefined);
  const { data: fileHandle } = useFileHandle(selectedFile?.id, needsHandle);
  const {
    data: viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
  } = useFileViewer(selectedFile?.id, hexEnabled);
  const textQuery = useTextPreview(selectedFile?.id, textEnabled);
  const imageQuery = useImagePreview(selectedFile?.id, imageEnabled);
  const mediaQuery = useMediaUrl(selectedFile?.id, mediaEnabled);
  const documentQuery = useDocumentPreview(selectedFile?.id, documentEnabled);
  const textPreview = textQuery.data;
  const imagePreview = imageQuery.data;
  const mediaUrl = mediaQuery.data;
  const documentPreview = documentQuery.data;
  const activePreviewQuery =
    viewerTab === 'text'
      ? textQuery
      : viewerTab === 'preview' && previewKind === 'image'
        ? imageQuery
        : viewerTab === 'preview' && (previewKind === 'video' || previewKind === 'audio')
          ? mediaQuery
          : viewerTab === 'preview' && previewKind === 'document'
            ? documentQuery
            : undefined;
  const previewError = (activePreviewQuery?.error as ApiErrorDto | null) ?? null;
  const onRetryPreview = activePreviewQuery
    ? () => {
        void activePreviewQuery.refetch();
      }
    : undefined;

  const selectRow = useCallback((row: ClassifiedFileRow) => {
    setSelected(row);
    setViewerTab(defaultTabFor(row));
  }, []);

  if (!board) {
    return <EmptyLine text="文件分类数据暂不可用。" />;
  }

  return (
    <div className="space-y-6">
      <SummaryStrip
        items={[
          ['文件总数', board.totalFiles.toString()],
          ['文件总大小', formatSize(board.totalSize)],
          ['魔数识别', board.magicClassifiedCount.toString()],
          ['元数据推断', board.metadataClassifiedCount.toString()],
          ['分类族', board.groups.length.toString()],
        ]}
      />

      {board.warnings?.length ? <WarningList warnings={board.warnings} /> : null}

      {board.groups.length === 0 ? (
        <EmptyLine text="未发现可分类文件。" />
      ) : (
        <div className="space-y-5">
          {board.groups.map((group) => {
            const iconKey = FAMILY_KEY[group.category] ?? 'Other';
            const Icon = CATEGORY_ICONS[iconKey] ?? CATEGORY_ICONS.Other;
            const color = CATEGORY_COLORS[iconKey] ?? CATEGORY_COLORS.Other;
            return (
              <CollapsibleSection
                key={group.category}
                className="rounded-none border border-forensics-border bg-forensics-surface p-4"
                contentClassName="mt-3 space-y-3"
                title={
                  <>
                    <Icon size={17} style={{ color }} />
                    <h4 className="text-[13px] font-light text-forensics-text">{group.displayName}</h4>
                    <span className="text-[11px] text-forensics-muted-lighter">
                      {group.fileCount} 个 · {formatSize(group.totalSize)}
                    </span>
                  </>
                }
                headerExtra={<StatusPill status={board.status} />}
              >
                {group.subcategories.map((sub) => (
                  <SubcategoryTable
                    key={sub.name}
                    sub={sub}
                    selectedFileId={selected?.fileId}
                    onSelect={selectRow}
                  />
                ))}
              </CollapsibleSection>
            );
          })}
        </div>
      )}

      <FilePreviewDialog
        open={Boolean(selected && selectedFile)}
        onOpenChange={(open) => {
          if (!open) setSelected(null);
        }}
        viewerTab={viewerTab}
        setViewerTab={setViewerTab}
        viewer={viewerTab === 'hex' ? viewer : undefined}
        fileHandle={fileHandle}
        previewKind={previewKind}
        onHexJumpInputChange={setJumpOffsetInput}
        onHexJump={jumpToOffset}
        onLoadNextHexRange={loadNextRange}
        onLoadPreviousHexRange={loadPreviousRange}
        textPreview={textPreview}
        imagePreview={imagePreview}
        mediaUrl={mediaUrl}
        documentPreview={documentPreview}
        selectedFile={selectedFile}
        previewError={previewError}
        onRetryPreview={onRetryPreview}
      />
    </div>
  );
}

function SubcategoryTable({
  sub,
  selectedFileId,
  onSelect,
}: {
  sub: ClassificationSubcategory;
  selectedFileId?: string;
  onSelect: (row: ClassifiedFileRow) => void;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center gap-2 text-[11px]">
        <span className="rounded-none bg-forensics-info-bg px-1.5 py-0.5 text-forensics-text">{sub.name}</span>
        <span className="text-forensics-muted-lighter">
          {sub.fileCount} 个 · {formatSize(sub.totalSize)}
          {sub.truncated ? ` · 抽样 ${sub.files.length} 个` : ''}
        </span>
      </div>
      <DenseTableFrame>
        <DenseDataTable
          rows={sub.files}
          columns={CLASSIFICATION_COLUMNS}
          getRowKey={(row) => row.fileId}
          selectedRowKey={selectedFileId}
          onRowClick={onSelect}
          emptyTitle="暂无文件"
          emptyDescription="该子分类当前没有可展示的抽样文件。"
        />
      </DenseTableFrame>
    </div>
  );
}
