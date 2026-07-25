import { useEffect, useMemo, useState } from 'react';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ViewerTabs } from '@/components/viewers/ViewerTabs';
import { HexViewer } from '@/components/viewers/HexViewer';
import { TextViewer } from '@/components/viewers/TextViewer';
import { ImageViewer } from '@/components/viewers/ImageViewer';
import { VideoViewer } from '@/components/viewers/VideoViewer';
import { AudioViewer } from '@/components/viewers/AudioViewer';
import { ViewerError } from '@/components/viewers/ViewerError';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import type {
  ApiErrorDto,
  DocumentPreviewResponse,
  DocumentSection,
  FileEntryRow,
  FileHexViewerState,
  ImagePreviewResponse,
  MediaPreview,
  TextPreviewResponse,
  ViewerHandle,
} from '@/types/models';

export type PreviewViewerTab = 'metadata' | 'text' | 'hex' | 'preview';

export type FilePreviewKind = 'image' | 'video' | 'audio' | 'document';

export interface FilePreviewTabsProps {
  viewerTab: PreviewViewerTab;
  setViewerTab: (tab: PreviewViewerTab) => void;
  viewer: FileHexViewerState | undefined;
  fileHandle?: ViewerHandle;
  previewKind?: FilePreviewKind;
  onHexJumpInputChange: (value: string) => void;
  onHexJump: (input?: string) => Promise<boolean>;
  onLoadNextHexRange: () => Promise<void> | void;
  onLoadPreviousHexRange: () => Promise<void> | void;
  textPreview: TextPreviewResponse | null | undefined;
  imagePreview: ImagePreviewResponse | null | undefined;
  mediaUrl: MediaPreview | null | undefined;
  documentPreview: DocumentPreviewResponse | null | undefined;
  selectedFile: FileEntryRow | undefined;
  previewError?: ApiErrorDto | null;
  onRetryPreview?: () => void;
}

function formatBytes(bytes?: number) {
  if (!bytes) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 || unitIndex === 0 ? Math.round(value) : value.toFixed(1)} ${units[unitIndex]}`;
}

function LargeMediaFallback({
  mediaType,
  previewBytes,
  totalBytes,
  protocol,
}: {
  mediaType: '视频' | '音频';
  previewBytes?: number;
  totalBytes?: number;
  protocol?: boolean;
}) {
  const title = protocol ? `大${mediaType}使用受控流式预览` : `大${mediaType}使用受控分块预览`;
  const detail = protocol
    ? '当前通过 evidence-media 受控协议提供按需 Range 读取；若播放器不支持该协议，可提取文件后在本机播放器查看。'
    : `当前只读取首个 ${formatBytes(previewBytes)} 片段进行安全预览；完整播放需要先使用右侧"提取文件"导出后在本机播放器查看。`;
  return (
    <div className="flex h-full items-center justify-center px-6 text-center text-forensics-text-tertiary">
      <div className="max-w-md space-y-2">
        <div className="text-[12px] font-light text-forensics-text">{title}</div>
        <div className="text-[11px] leading-5">{detail}</div>
        <div className="font-mono text-[10px] text-forensics-muted-light">
          total={formatBytes(totalBytes)} / source=opaque handle
        </div>
      </div>
    </div>
  );
}

function HexPreviewContent({
  viewer,
  onHexJumpInputChange,
  onHexJump,
  onLoadNextHexRange,
  onLoadPreviousHexRange,
}: Pick<
  FilePreviewTabsProps,
  'viewer' | 'onHexJumpInputChange' | 'onHexJump' | 'onLoadNextHexRange' | 'onLoadPreviousHexRange'
>) {
  const [inspectorExpanded, setInspectorExpanded] = useState(false);

  useEffect(() => {
    setInspectorExpanded(false);
  }, [viewer?.handle.handleId]);

  if (!viewer || (viewer.rawBytes.length === 0 && viewer.lines.length === 0)) {
    return <div className="text-forensics-muted">选择文件后显示十六进制预览。</div>;
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1">
        <HexViewer
          lines={viewer.lines}
          rawBytes={viewer.rawBytes}
          baseOffset={viewer.baseOffset}
          fileSize={viewer.fileSize}
          activeOffset={viewer.activeOffset}
          loadedRanges={viewer.loadedRanges}
          onNeedMoreRange={(direction) => {
            if (direction === 'next') {
              void onLoadNextHexRange();
            } else {
              void onLoadPreviousHexRange();
            }
          }}
        />
      </div>
      {viewer.error ? (
        <div className="mt-2 shrink-0 text-[10px] text-forensics-error-text">{viewer.error}</div>
      ) : null}
      <div className="mt-2 shrink-0 border border-forensics-border bg-forensics-panel">
        <Button
          type="button"
          variant="forensicsGhost"
          size="xs"
          onClick={() => setInspectorExpanded((value) => !value)}
          className="h-auto w-full justify-between px-3 py-2 text-left text-[10px]"
        >
          <div className="flex min-w-0 items-center gap-3">
            <span className="font-light text-forensics-text">
              {viewer.mode === 'full' ? '完整 Hex 预览' : '分段只读浏览'}
            </span>
            <span>
              已加载{' '}
              {formatBytes(
                viewer.loadedRanges.reduce((total, range) => total + (range.end - range.start), 0),
              )}{' '}
              / {formatBytes(viewer.fileSize)}
            </span>
            <span>offset={viewer.activeOffset.toString(16).toUpperCase().padStart(8, '0')}</span>
          </div>
          {inspectorExpanded ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
        </Button>
        {inspectorExpanded ? (
          <div className="border-t border-forensics-border px-3 py-2 text-[10px] text-forensics-muted">
            <div className="flex items-center gap-2">
              <Input
                value={viewer.jumpOffsetInput}
                onChange={(event) => onHexJumpInputChange(event.target.value)}
                variant="mono"
                inputSize="inline"
                className="w-32"
                placeholder="0x0"
              />
              <Button
                type="button"
                variant="forensicsOutline"
                size="compact"
                onClick={() => {
                  void onHexJump();
                }}
              >
                跳转
              </Button>
            </div>
            <div className="mt-2">
              {viewer.mode === 'chunked'
                ? '分段只读浏览，支持纵向滚动与 offset 跳转，不会一次性加载整个文件。'
                : '完整 Hex 预览，可直接纵向滚动浏览全部内容。'}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function DocumentSectionBody({ section }: { section: DocumentSection }) {
  const table = useMemo(() => {
    if (!section.table) return undefined;
    const columns: DenseColumn<{ key: string; cells: string[] }>[] = section.table.columns.map(
      (title, index) => ({
        key: `c${index}`,
        title,
        className: 'min-w-[120px]',
        render: (row) => row.cells[index] ?? '',
      }),
    );
    const rows = section.table.rows.map((cells, index) => ({ key: String(index), cells }));
    return { columns, rows };
  }, [section.table]);

  if (table) {
    return (
      <div className="flex max-h-[min(60vh,560px)] min-h-0 flex-col overflow-hidden">
        <DenseDataTable
          rows={table.rows}
          columns={table.columns}
          getRowKey={(row) => row.key}
          emptyTitle="空表格"
          emptyDescription="该段没有可展示的数据行。"
        />
      </div>
    );
  }
  return (
    <pre className="whitespace-pre-wrap break-all font-mono text-[11px] leading-5 text-forensics-text-secondary">
      {section.lines.join('\n')}
    </pre>
  );
}

function DocumentPreviewContent({
  documentPreview,
}: {
  documentPreview: DocumentPreviewResponse;
}) {
  return (
    <div className="h-full overflow-auto px-3 py-2">
      <div className="mb-2 flex items-center gap-2 text-[11px] text-forensics-muted">
        <span className="rounded bg-forensics-info-bg px-1.5 py-0.5 uppercase">
          {documentPreview.kind}
        </span>
        <span>{documentPreview.summary}</span>
        {documentPreview.truncated ? <span>（内容已按预览上限截断）</span> : null}
      </div>
      {documentPreview.warnings?.length ? (
        <div className="mb-2 text-[10px] text-forensics-warning-text">
          {documentPreview.warnings.slice(0, 3).map((warning) => (
            <div key={warning}>{warning}</div>
          ))}
        </div>
      ) : null}
      {documentPreview.sections.length === 0 ? (
        <div className="flex h-32 items-center justify-center text-forensics-muted-light">
          未提取到可读文本内容
        </div>
      ) : (
        documentPreview.sections.map((section, index) => (
          <div key={`${section.title}-${index}`} className="mb-3">
            <div className="mb-1 border-b border-forensics-border text-[11px] font-medium text-forensics-text">
              {section.title}
            </div>
            <DocumentSectionBody section={section} />
          </div>
        ))
      )}
    </div>
  );
}

function MediaPreviewContent({
  mediaType,
  mediaUrl,
  fileName,
}: {
  mediaType: 'video' | 'audio';
  mediaUrl: MediaPreview | null | undefined;
  fileName?: string;
}) {
  const label = mediaType === 'video' ? '视频' : '音频';
  const Viewer = mediaType === 'video' ? VideoViewer : AudioViewer;
  if (mediaUrl?.url) {
    const player = <Viewer src={mediaUrl.url} mimeType={mediaUrl.mimeType} fileName={fileName} />;
    if (mediaUrl.previewMode === 'rangeFallback' || mediaUrl.previewMode === 'range') {
      return (
        <div className="flex h-full flex-col">
          <div className="min-h-0 flex-1">{player}</div>
          <div className="border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
            受控分块预览: 已读取 {formatBytes(mediaUrl.previewBytes)}，完整播放请提取文件后查看。
          </div>
        </div>
      );
    }
    if (mediaUrl.previewMode === 'protocol') {
      return (
        <div className="flex h-full flex-col">
          <div className="min-h-0 flex-1">{player}</div>
          <div className="border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
            受控流式预览: evidence-media range 读取，不暴露宿主证据路径。
          </div>
        </div>
      );
    }
    return player;
  }
  if (mediaUrl?.canReadRanges || mediaUrl?.handleId) {
    return (
      <LargeMediaFallback
        mediaType={label}
        previewBytes={mediaUrl.previewBytes}
        totalBytes={mediaUrl.size}
        protocol={mediaUrl.previewMode === 'protocol'}
      />
    );
  }
  return (
    <div className="flex h-full items-center justify-center text-forensics-muted-light">
      加载{label}预览...
    </div>
  );
}

function KindPreviewContent({
  previewKind,
  imagePreview,
  mediaUrl,
  documentPreview,
  selectedFile,
}: Pick<
  FilePreviewTabsProps,
  'previewKind' | 'imagePreview' | 'mediaUrl' | 'documentPreview' | 'selectedFile'
>) {
  if (previewKind === 'image') {
    if (imagePreview?.dataUrl) {
      return (
        <ImageViewer
          src={imagePreview.dataUrl}
          mimeType={imagePreview.mimeType}
          fileName={selectedFile?.name}
        />
      );
    }
    return (
      <div className="flex h-full items-center justify-center text-forensics-muted-light">
        加载图片预览...
      </div>
    );
  }
  if (previewKind === 'document') {
    if (documentPreview) {
      return <DocumentPreviewContent documentPreview={documentPreview} />;
    }
    return (
      <div className="flex h-full items-center justify-center text-forensics-muted-light">
        加载文档预览...
      </div>
    );
  }
  if (previewKind === 'video' || previewKind === 'audio') {
    return (
      <MediaPreviewContent
        mediaType={previewKind}
        mediaUrl={mediaUrl}
        fileName={selectedFile?.name}
      />
    );
  }
  return (
    <div className="flex h-full items-center justify-center text-forensics-muted-light">
      选择图片、视频或音频文件后显示预览
    </div>
  );
}

/**
 * Shared viewer-tab preview surface (hex / text / kind preview / metadata).
 * Used by the file-browser bottom panel and the classification preview dialog
 * so both stay on the same public primitives.
 */
export function FilePreviewTabs({
  viewerTab,
  setViewerTab,
  viewer,
  fileHandle,
  previewKind,
  onHexJumpInputChange,
  onHexJump,
  onLoadNextHexRange,
  onLoadPreviousHexRange,
  textPreview,
  imagePreview,
  mediaUrl,
  documentPreview,
  selectedFile,
  previewError,
  onRetryPreview,
}: FilePreviewTabsProps) {
  return (
    <div className="flex h-full min-h-0 flex-col bg-forensics-surface">
      {previewError ? <ViewerError error={previewError} onRetry={onRetryPreview} /> : null}
      {!previewError && selectedFile?.encrypted ? (
        <div className="flex items-center gap-2 border-b border-forensics-warning-border bg-forensics-warning-bg px-3 py-1.5 text-[11px] text-forensics-warning-text">
          <span className="font-light">EFS Encrypted</span>
          <span className="text-forensics-warning-text">
            This file is encrypted with NTFS Encrypting File System. Content cannot be decrypted
            without the private key.
          </span>
        </div>
      ) : null}
      <div className="min-h-0 flex-1">
        <ViewerTabs
          value={viewerTab}
          onValueChange={(value) => setViewerTab(value as PreviewViewerTab)}
          tabs={[
            {
              value: 'hex',
              label: '十六进制',
              contentClassName: 'min-h-0 flex-1 overflow-hidden p-0 text-[11px]',
              content: (
                <HexPreviewContent
                  viewer={viewer}
                  onHexJumpInputChange={onHexJumpInputChange}
                  onHexJump={onHexJump}
                  onLoadNextHexRange={onLoadNextHexRange}
                  onLoadPreviousHexRange={onLoadPreviousHexRange}
                />
              ),
            },
            {
              value: 'text',
              label: '文本',
              content:
                textPreview && !textPreview.isBinary ? (
                  <TextViewer
                    content={textPreview.content}
                    encoding={textPreview.encoding}
                    extension={selectedFile?.ext}
                    isTruncated={textPreview.isTruncated}
                  />
                ) : (
                  <div className="flex h-full items-center justify-center text-forensics-muted-light">
                    {textPreview?.isBinary ? '二进制文件，无法预览文本' : '选择文本文件后显示预览'}
                  </div>
                ),
            },
            {
              value: 'preview',
              label: '预览',
              content: (
                <KindPreviewContent
                  previewKind={previewKind}
                  imagePreview={imagePreview}
                  mediaUrl={mediaUrl}
                  documentPreview={documentPreview}
                  selectedFile={selectedFile}
                />
              ),
            },
            {
              value: 'metadata',
              label: '元数据',
              content: (
                <div className="space-y-2 font-mono text-[11px] text-forensics-text-secondary">
                  <div>handle_id: {fileHandle?.handleId ?? viewer?.handle.handleId ?? '-'}</div>
                  <div>
                    size: {fileHandle?.size ?? viewer?.handle.size ?? selectedFile?.size ?? '-'}
                  </div>
                  <div>mime: {fileHandle?.mime ?? viewer?.handle.mime ?? '-'}</div>
                  <div>path: {selectedFile?.path ?? '-'}</div>
                </div>
              ),
            },
          ]}
        />
      </div>
    </div>
  );
}
