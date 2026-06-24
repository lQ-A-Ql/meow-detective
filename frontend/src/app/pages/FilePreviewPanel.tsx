import { useNavigate } from 'react-router';
import { ViewerTabs } from '@/components/viewers/ViewerTabs';
import { HexViewer } from '@/components/viewers/HexViewer';
import { TextViewer } from '@/components/viewers/TextViewer';
import { ImageViewer } from '@/components/viewers/ImageViewer';
import { VideoViewer } from '@/components/viewers/VideoViewer';
import { AudioViewer } from '@/components/viewers/AudioViewer';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { useSelectionStore } from '@/stores/selection-store';
import type {
  FileEntryRow,
  ViewerHandle,
  ViewerRangeResponse,
  TextPreviewResponse,
  ImagePreviewResponse,
  MediaUrl,
} from '@/types/models';
import type { UseMutationResult } from '@tanstack/react-query';

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

// Viewer data types
interface ViewerData {
  handle: ViewerHandle;
  range: ViewerRangeResponse;
}

export interface FilePreviewPanelProps {
  viewerTab: 'metadata' | 'text' | 'hex' | 'preview';
  setViewerTab: (tab: 'metadata' | 'text' | 'hex' | 'preview') => void;
  viewer: ViewerData | undefined;
  textPreview: TextPreviewResponse | null | undefined;
  imagePreview: ImagePreviewResponse | null | undefined;
  mediaUrl: MediaUrl | null | undefined;
  selectedFile: FileEntryRow | undefined;
  activeDirectoryPath: string | undefined;
  currentDirectory: { name: string } | undefined;
  extractFile: UseMutationResult<unknown, Error, FileEntryRow>;
}

export function FilePreviewPanel({
  viewerTab,
  setViewerTab,
  viewer,
  textPreview,
  imagePreview,
  mediaUrl,
  selectedFile,
  activeDirectoryPath,
  currentDirectory,
  extractFile,
}: FilePreviewPanelProps) {
  const navigate = useNavigate();
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);

  return (
    <>
      <div className="h-72 bg-[#fcfcfc] shrink-0 min-h-0">
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
              content: viewer?.range.lines?.length ? (
                <HexViewer lines={viewer.range.lines} />
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
                const mime = viewer?.handle.mime?.toLowerCase() ?? '';
                const ext = (selectedFile?.ext ?? selectedFile?.name.split('.').pop() ?? '').toLowerCase().replace(/^\./, '');
                const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'];
                const videoExts = ['mp4', 'webm', 'avi', 'mkv'];
                const audioExts = ['mp3', 'wav', 'flac', 'aac', 'ogg'];
                
                // 图片预览
                if (mime.startsWith('image/') || imageExts.includes(ext)) {
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
                if (mime.startsWith('video/') || videoExts.includes(ext)) {
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
                if (mime.startsWith('audio/') || audioExts.includes(ext)) {
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
                    handle_id: {viewer?.handle.handleId ?? '-'}
                  </div>
                  <div>size: {viewer?.handle.size ?? '-'}</div>
                  <div>mime: {viewer?.handle.mime ?? '-'}</div>
                  <div>path: {selectedFile?.path ?? '-'}</div>
                </div>
              ),
            },
          ]}
        />
      </div>

      <InspectorPane
        title="对象检查器"
        subtitle={
          selectedFile
            ? `已选对象 ${selectedFile.name}`
            : '未选中文件对象'
        }
        widthClassName="w-80"
      >
        <div className="space-y-5">
          <InspectorSection title="对象标识">
            <InspectorValue
              value={selectedFile?.name ?? '-'}
              mono
              strong
            />
          </InspectorSection>

          <InspectorSection title="来源路径">
            <InspectorValue
              value={
                selectedFile?.path ??
                activeDirectoryPath ??
                currentDirectory?.name ??
                '-'
              }
              mono
            />
          </InspectorSection>

          <InspectorSection title="时间戳 (MACB)">
            <div className="font-mono text-[11px] grid grid-cols-[30px_1fr] gap-1">
              <div className="text-[#888]">M</div>
              <div className="text-[#333]">
                {selectedFile?.modifiedAt ?? '-'}
              </div>
              <div className="text-[#888]">A</div>
              <div className="text-[#333]">
                {selectedFile?.accessedAt ?? '-'}
              </div>
              <div className="text-[#888]">C</div>
              <div className="text-[#333]">
                {selectedFile?.changedAt ?? '-'}
              </div>
              <div className="text-[#888]">B</div>
              <div className="text-[#333]">
                {selectedFile?.createdAt ?? '-'}
              </div>
            </div>
          </InspectorSection>

          <InspectorSection title="摘要字段">
            <InspectorValue
              value={selectedFile?.hashSha256 ?? '-'}
              mono
            />
          </InspectorSection>

          <InspectorSection title="对象状态">
            <div className="font-mono text-[11px] grid grid-cols-[60px_1fr] gap-1">
              <div className="text-[#888]">deleted</div>
              <div className="text-[#333]">{selectedFile?.deleted ? 'true' : 'false'}</div>
              <div className="text-[#888]">hidden</div>
              <div className="text-[#333]">{selectedFile?.hidden ? 'true' : 'false'}</div>
              <div className="text-[#888]">system</div>
              <div className="text-[#333]">{selectedFile?.system ? 'true' : 'false'}</div>
            </div>
          </InspectorSection>

          <InspectorSection title="操作">
            <div className="flex flex-col gap-2">
              <button
                type="button"
                onClick={() => {
                  if (selectedFile) {
                    extractFile.mutate(selectedFile);
                  }
                }}
                disabled={!selectedFile || extractFile.isPending}
                className="w-full border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] py-1.5 text-center text-[11px] rounded-[2px] cursor-pointer font-medium disabled:opacity-50"
              >
                {extractFile.isPending ? '提取中...' : '提取文件'}
              </button>
              <button
                onClick={() => {
                  if (selectedFile) {
                    setSelectedTimelineId(selectedFile.id);
                    navigate('/timeline');
                  }
                }}
                className="w-full border border-transparent text-[#666] hover:text-[#111] py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline"
              >
                在时间线中查看
              </button>
            </div>
          </InspectorSection>
        </div>
      </InspectorPane>
    </>
  );
}
