import { useCallback, useEffect, useMemo, useState } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { ViewerTabFrame } from '@/components/tabs/ViewerTabFrame';
import { HexViewer } from '@/components/viewers/HexViewer';
import { TextViewer } from '@/components/viewers/TextViewer';
import { ImageViewer } from '@/components/viewers/ImageViewer';
import { VideoViewer } from '@/components/viewers/VideoViewer';
import { AudioViewer } from '@/components/viewers/AudioViewer';
import { ViewerError } from '@/components/viewers/ViewerError';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { BrandWatermark } from '@/components/brand';
import { ImageMetadataPanel } from '@/components/preview/ImageMetadataPanel';
import type {
  ApiErrorDto,
  DocumentPreviewResponse,
  DocumentSection,
  FileEntryRow,
  FileHexViewerState,
  ImagePreviewResponse,
  ImageMetadata,
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
  imageMetadata?: ImageMetadata | null;
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
  mediaType: 'video' | 'audio';
  previewBytes?: number;
  totalBytes?: number;
  protocol?: boolean;
}) {
  const { t } = useTranslation();
  const titleKey = protocol ? 'preview.media.largeTitleProtocol' : 'preview.media.largeTitleChunked';
  const detailKey = protocol ? 'preview.media.largeDetailProtocol' : 'preview.media.largeDetailChunked';
  const title = t(titleKey, { type: mediaType === 'video' ? t('media.video') : t('media.audio') });
  const detail = t(detailKey, protocol ? { bytes: formatBytes(previewBytes) } : undefined);
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
  const { t } = useTranslation();
  const [inspectorExpanded, setInspectorExpanded] = useState(false);
  const handleNeedMoreRange = useCallback(
    (direction: 'previous' | 'next') => {
      if (direction === 'next') {
        void onLoadNextHexRange();
      } else {
        void onLoadPreviousHexRange();
      }
    },
    [onLoadNextHexRange, onLoadPreviousHexRange],
  );

  useEffect(() => {
    setInspectorExpanded(false);
  }, [viewer?.handle.handleId]);

  if (!viewer || (viewer.rawBytes.length === 0 && viewer.lines.length === 0)) {
    return <div className="text-forensics-muted">{t('viewer.hex.empty')}</div>;
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
          onNeedMoreRange={handleNeedMoreRange}
        />
      </div>
      {viewer.error ? (
        <div className="mt-2 shrink-0 text-[10px] text-forensics-error-text">{viewer.error}</div>
      ) : null}
      <div className="mt-2 shrink-0 border border-forensics-border bg-forensics-panel">
        <Button
          type="button"
          variant="forensicsGhost"
          shape="row"
          aria-expanded={inspectorExpanded}
          onClick={() => setInspectorExpanded((value) => !value)}
          className="h-auto w-full justify-between gap-3 rounded-none px-3 py-2 text-left text-[10px]"
        >
          <div className="flex min-w-0 items-center gap-3">
            <span className="font-light text-forensics-text">
              {viewer.mode === 'full' ? t('preview.hex.fullMode') : t('preview.hex.chunkedMode')}
            </span>
            <span>
              {t('preview.hex.loaded')}{' '}
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
              <Tooltip content={t('preview.hex.jump')}><Button aria-label={t('preview.hex.jump')}
                type="button"
                variant="forensicsOutline"
                size="compact"
                onClick={() => {
                  void onHexJump();
                }}
              ><ActionIcon name="chevron-left" /></Button></Tooltip>
            </div>
            <div className="mt-2">
              {viewer.mode === 'chunked'
                ? t('preview.hex.chunkedHint')
                : t('preview.hex.fullHint')}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function DocumentSectionBody({ section }: { section: DocumentSection }) {
  const { t } = useTranslation();
  const table = useMemo(() => {
    if (!section.table) return undefined;
    const columns: DenseColumn<{ key: string; cells: string[] }>[] = section.table.columns.map(
      (title, index) => ({
        key: `c${index}`,
        title,
        render: (row) => row.cells[index] ?? '',
      }),
    );
    const rows = section.table.rows.map((cells, index) => ({ key: String(index), cells }));
    return { columns, rows };
  }, [section.table]);

  if (table) {
    return (
      <DenseDataTableFrame
        rowCount={table.rows.length}
        maxHeight="compact"
        variant="plain"
      >
        <DenseDataTable
          rows={table.rows}
          columns={table.columns}
          getRowKey={(row) => row.key}
          horizontalScroll
          minColumnWidth={140}
          emptyTitle={t('preview.document.tableEmptyTitle')}
          emptyDescription={t('preview.document.tableEmptyDescription')}
        />
      </DenseDataTableFrame>
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
  const { t } = useTranslation();
  return (
    <ScrollArea className="h-full" viewportClassName="px-3 py-2">
      <div className="relative isolate min-h-full">
        <BrandWatermark
          motif="documentPaw"
          className="absolute bottom-36 -right-2 h-24 opacity-[0.02]"
        />
        <div className="relative">
          <div className="mb-2 flex items-center gap-2 text-[11px] text-forensics-muted">
            <span className="rounded bg-forensics-info-bg px-1.5 py-0.5 uppercase">
              {documentPreview.kind}
            </span>
            <span>{documentPreview.summary}</span>
            {documentPreview.truncated ? <span>{t('preview.document.truncated')}</span> : null}
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
              {t('preview.document.empty')}
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
      </div>
    </ScrollArea>
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
  const { t } = useTranslation();
  const label = mediaType === 'video' ? t('media.video') : t('media.audio');
  const Viewer = mediaType === 'video' ? VideoViewer : AudioViewer;
  if (mediaUrl?.url) {
    const player = <Viewer src={mediaUrl.url} mimeType={mediaUrl.mimeType} fileName={fileName} />;
    if (mediaUrl.previewMode === 'rangeFallback' || mediaUrl.previewMode === 'range') {
      return (
        <div className="flex h-full flex-col">
          <div className="min-h-0 flex-1">{player}</div>
          <div className="border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
            {t('preview.media.rangeBanner', { bytes: formatBytes(mediaUrl.previewBytes) })}
          </div>
        </div>
      );
    }
    if (mediaUrl.previewMode === 'protocol') {
      return (
        <div className="flex h-full flex-col">
          <div className="min-h-0 flex-1">{player}</div>
          <div className="border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
            {t('preview.media.protocolBanner')}
          </div>
        </div>
      );
    }
    return player;
  }
  if (mediaUrl?.canReadRanges || mediaUrl?.handleId) {
    return (
      <LargeMediaFallback
        mediaType={mediaType}
        previewBytes={mediaUrl.previewBytes}
        totalBytes={mediaUrl.size}
        protocol={mediaUrl.previewMode === 'protocol'}
      />
    );
  }
  return (
    <div className="flex h-full items-center justify-center text-forensics-muted-light">
      {t('preview.media.loading', { type: label })}
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
  const { t } = useTranslation();
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
        {t('preview.image.loading')}
      </div>
    );
  }
  if (previewKind === 'document') {
    if (documentPreview) {
      return <DocumentPreviewContent documentPreview={documentPreview} />;
    }
    return (
      <div className="flex h-full items-center justify-center text-forensics-muted-light">
        {t('preview.document.loading')}
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
      {t('preview.selectMedia')}
    </div>
  );
}

/** Shared viewer-tab preview surface (hex / text / kind preview / metadata) for the
 * file-browser bottom panel and the classification preview dialog. */
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
  imageMetadata,
  mediaUrl,
  documentPreview,
  selectedFile,
  previewError,
  onRetryPreview,
}: FilePreviewTabsProps) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full min-h-0 flex-col bg-forensics-surface">
      {previewError ? <ViewerError error={previewError} onRetry={onRetryPreview} /> : null}
      {!previewError && selectedFile?.encrypted ? (
        <div className="flex items-center gap-2 border-b border-forensics-warning-border bg-forensics-warning-bg px-3 py-1.5 text-[11px] text-forensics-warning-text">
          <span className="font-light">{t('viewer.hex.efsTitle')}</span>
          <span className="text-forensics-warning-text">{t('viewer.hex.efsDetail')}</span>
        </div>
      ) : null}
      <div className="min-h-0 flex-1">
        <ViewerTabFrame
          value={viewerTab}
          onValueChange={(value) => setViewerTab(value as PreviewViewerTab)}
          tabs={[
            {
              value: 'hex',
              label: t('preview.tabs.hex'),
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
              label: t('preview.tabs.text'),
              content:
                textPreview && !textPreview.isBinary ? (
                  <TextViewer
                    documentKey={selectedFile?.id}
                    content={textPreview.content}
                    encoding={textPreview.encoding}
                    extension={selectedFile?.ext}
                    isTruncated={textPreview.isTruncated}
                  />
                ) : (
                  <div className="flex h-full items-center justify-center text-forensics-muted-light">
                    {textPreview?.isBinary ? t('preview.binaryText') : t('preview.selectText')}
                  </div>
                ),
            },
            {
              value: 'preview',
              label: t('preview.tabs.preview'),
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
              label: t('preview.tabs.metadata'),
              content: (
                <div className="space-y-2 font-mono text-[11px] text-forensics-text-secondary">
                  <div>handle_id: {fileHandle?.handleId ?? viewer?.handle.handleId ?? '-'}</div>
                  <div>
                    size: {fileHandle?.size ?? viewer?.handle.size ?? selectedFile?.size ?? '-'}
                  </div>
                  <div>mime: {fileHandle?.mime ?? viewer?.handle.mime ?? '-'}</div>
                  <div>path: {selectedFile?.path ?? '-'}</div>
                  {previewKind === 'image' ? <ImageMetadataPanel metadata={imageMetadata} /> : null}
                </div>
              ),
            },
          ]}
        />
      </div>
    </div>
  );
}
