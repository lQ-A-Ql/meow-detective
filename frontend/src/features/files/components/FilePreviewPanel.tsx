import { ViewerTabs } from '@/components/viewers/ViewerTabs';
import { HexViewer } from '@/components/viewers/HexViewer';
import { TextViewer } from '@/components/viewers/TextViewer';
import { ImageViewer } from '@/components/viewers/ImageViewer';
import { VideoViewer } from '@/components/viewers/VideoViewer';
import { AudioViewer } from '@/components/viewers/AudioViewer';
import { ViewerError } from '@/components/viewers/ViewerError';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { useResizableHeight } from '@/hooks/use-resizable-height';
import type {
  ApiErrorDto,
  FileHexViewerState,
  FileEntryRow,
  TextPreviewResponse,
  ImagePreviewResponse,
  MediaPreview,
  ViewerHandle,
} from '@/types/models';

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
    <div className="flex h-full items-center justify-center px-6 text-center text-[#555]">
      <div className="max-w-md space-y-2">
        <div className="text-[12px] font-semibold text-[#222]">
          {title}
        </div>
        <div className="text-[11px] leading-5">
          {detail}
        </div>
        <div className="font-mono text-[10px] text-[#888]">
          total={formatBytes(totalBytes)} / source=opaque handle
        </div>
      </div>
    </div>
  );
}

export type FilePreviewKind = 'image' | 'video' | 'audio';

export interface FilePreviewPanelProps {
  viewerTab: 'metadata' | 'text' | 'hex' | 'preview';
  setViewerTab: (tab: 'metadata' | 'text' | 'hex' | 'preview') => void;
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
  selectedFile: FileEntryRow | undefined;
  previewError?: ApiErrorDto | null;
  onRetryPreview?: () => void;
}

export function FilePreviewPanel({
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
  selectedFile,
  previewError,
  onRetryPreview,
}: FilePreviewPanelProps) {
  const [hexInspectorExpanded, setHexInspectorExpanded] = useState(false);
  const { height: previewHeight, isResizing: isResizingPreview, onResizeStart: onPreviewResizeStart } = useResizableHeight({
    defaultHeight: 288,
    minHeight: 160,
    maxHeight: 600,
    storageKey: 'filePreviewHeight',
  });

  useEffect(() => {
    setHexInspectorExpanded(false);
  }, [viewerTab, selectedFile?.id]);

  return (
    <div
      className="bg-[#fcfcfc] shrink-0 min-h-0 flex flex-col"
      style={{ height: `${previewHeight}px` }}
    >
      <div
        className={`shrink-0 h-1 cursor-row-resize transition-colors ${
          isResizingPreview ? 'bg-blue-400' : 'hover:bg-blue-200'
        }`}
        onMouseDown={onPreviewResizeStart}
        title="拖拽调整预览区高度"
      />
      {previewError && (
        <ViewerError error={previewError} onRetry={onRetryPreview} />
      )}
      {!previewError && selectedFile?.encrypted && (
        <div className="flex items-center gap-2 bg-amber-50 border-b border-amber-200 px-3 py-1.5 text-[11px] text-amber-800">
          <span className="font-semibold">EFS Encrypted</span>
          <span className="text-amber-600">
            This file is encrypted with NTFS Encrypting File System. Content cannot be decrypted without the private key.
          </span>
        </div>
      )}
      <div className="min-h-0 flex-1">
        <ViewerTabs
        value={viewerTab}
        onValueChange={(value) =>
          setViewerTab(
            value as 'metadata' | 'text' | 'hex' | 'preview'
          )
        }
        tabs={[
          {
            value: 'hex',
            label: '十六进制',
            contentClassName: 'min-h-0 flex-1 overflow-hidden p-0 text-[11px]',
            content: viewer && (viewer.rawBytes.length > 0 || viewer.lines.length > 0) ? (
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
                  <div className="mt-2 shrink-0 text-[10px] text-red-600">{viewer.error}</div>
                ) : null}
                <div className="mt-2 shrink-0 border border-[#e0e0e0] bg-[#fafafa]">
                  <Button
                    type="button"
                    variant="forensicsGhost"
                    size="xs"
                    onClick={() => setHexInspectorExpanded((value) => !value)}
                    className="h-auto w-full justify-between px-3 py-2 text-left text-[10px]"
                  >
                    <div className="flex min-w-0 items-center gap-3">
                      <span className="font-semibold text-[#222]">
                        {viewer.mode === 'full' ? '完整 Hex 预览' : '分段只读浏览'}
                      </span>
                      <span>
                        已加载 {formatBytes(viewer.loadedRanges.reduce((total, range) => total + (range.end - range.start), 0))} / {formatBytes(viewer.fileSize)}
                      </span>
                      <span>offset={viewer.activeOffset.toString(16).toUpperCase().padStart(8, '0')}</span>
                    </div>
                    {hexInspectorExpanded ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
                  </Button>
                  {hexInspectorExpanded ? (
                    <div className="border-t border-[#e0e0e0] px-3 py-2 text-[10px] text-[#666]">
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
                          onClick={() => { void onHexJump(); }}
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
            ) : (
              <div className="text-[#666]">
                选择文件后显示十六进制预览。
              </div>
            ),
          },
          {
            value: 'text',
            label: '文本',
            content: textPreview && !textPreview.isBinary ? (
              <TextViewer
                content={textPreview.content}
                encoding={textPreview.encoding}
                extension={selectedFile?.ext}
                isTruncated={textPreview.isTruncated}
              />
            ) : (
              <div className="flex items-center justify-center h-full text-[#888]">
                {textPreview?.isBinary ? '二进制文件，无法预览文本' : '选择文本文件后显示预览'}
              </div>
            ),
          },
          {
            value: 'preview',
            label: '预览',
            content: (() => {
              
              // 图片预览
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
                return <div className="flex items-center justify-center h-full text-[#888]">加载图片预览...</div>;
              }
              
              // 视频预览
              if (previewKind === 'video') {
                if (mediaUrl?.url) {
                  return mediaUrl.previewMode === 'rangeFallback' || mediaUrl.previewMode === 'range' ? (
                    <div className="h-full flex flex-col">
                      <div className="flex-1 min-h-0">
                        <VideoViewer
                          src={mediaUrl.url}
                          mimeType={mediaUrl.mimeType}
                          fileName={selectedFile?.name}
                        />
                      </div>
                      <div className="border-t border-[#e0e0e0] bg-[#f8f8f8] px-3 py-1 text-[10px] text-[#666]">
                        受控分块预览: 已读取 {formatBytes(mediaUrl.previewBytes)}，完整播放请提取文件后查看。
                      </div>
                    </div>
                  ) : mediaUrl.previewMode === 'protocol' ? (
                    <div className="h-full flex flex-col">
                      <div className="flex-1 min-h-0">
                        <VideoViewer
                          src={mediaUrl.url}
                          mimeType={mediaUrl.mimeType}
                          fileName={selectedFile?.name}
                        />
                      </div>
                      <div className="border-t border-[#e0e0e0] bg-[#f8f8f8] px-3 py-1 text-[10px] text-[#666]">
                        受控流式预览: evidence-media range 读取，不暴露宿主证据路径。
                      </div>
                    </div>
                  ) : (
                    <VideoViewer
                      src={mediaUrl.url}
                      mimeType={mediaUrl.mimeType}
                      fileName={selectedFile?.name}
                    />
                  );
                }
                if (mediaUrl?.canReadRanges || mediaUrl?.handleId) {
                  return (
                    <LargeMediaFallback
                      mediaType="视频"
                      previewBytes={mediaUrl.previewBytes}
                      totalBytes={mediaUrl.size}
                      protocol={mediaUrl.previewMode === 'protocol'}
                    />
                  );
                }
                return <div className="flex items-center justify-center h-full text-[#888]">加载视频预览...</div>;
              }
              
              // 音频预览
              if (previewKind === 'audio') {
                if (mediaUrl?.url) {
                  return mediaUrl.previewMode === 'rangeFallback' || mediaUrl.previewMode === 'range' ? (
                    <div className="h-full flex flex-col">
                      <div className="flex-1 min-h-0">
                        <AudioViewer
                          src={mediaUrl.url}
                          mimeType={mediaUrl.mimeType}
                          fileName={selectedFile?.name}
                        />
                      </div>
                      <div className="border-t border-[#333] bg-[#111] px-3 py-1 text-[10px] text-[#aaa]">
                        受控分块预览: 已读取 {formatBytes(mediaUrl.previewBytes)}，完整播放请提取文件后查看。
                      </div>
                    </div>
                  ) : mediaUrl.previewMode === 'protocol' ? (
                    <div className="h-full flex flex-col">
                      <div className="flex-1 min-h-0">
                        <AudioViewer
                          src={mediaUrl.url}
                          mimeType={mediaUrl.mimeType}
                          fileName={selectedFile?.name}
                        />
                      </div>
                      <div className="border-t border-[#333] bg-[#111] px-3 py-1 text-[10px] text-[#aaa]">
                        受控流式预览: evidence-media range 读取，不暴露宿主证据路径。
                      </div>
                    </div>
                  ) : (
                    <AudioViewer
                      src={mediaUrl.url}
                      mimeType={mediaUrl.mimeType}
                      fileName={selectedFile?.name}
                    />
                  );
                }
                if (mediaUrl?.canReadRanges || mediaUrl?.handleId) {
                  return (
                    <LargeMediaFallback
                      mediaType="音频"
                      previewBytes={mediaUrl.previewBytes}
                      totalBytes={mediaUrl.size}
                      protocol={mediaUrl.previewMode === 'protocol'}
                    />
                  );
                }
                return <div className="flex items-center justify-center h-full text-[#888]">加载音频预览...</div>;
              }
              
              // 默认
              return (
                <div className="flex items-center justify-center h-full text-[#888]">
                  选择图片、视频或音频文件后显示预览
                </div>
              );
            })(),
          },
          {
            value: 'metadata',
            label: '元数据',
            content: (
              <div className="space-y-2 font-mono text-[11px] text-[#444]">
                <div>
                  handle_id: {fileHandle?.handleId ?? viewer?.handle.handleId ?? '-'}
                </div>
                <div>size: {fileHandle?.size ?? viewer?.handle.size ?? selectedFile?.size ?? '-'}</div>
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
