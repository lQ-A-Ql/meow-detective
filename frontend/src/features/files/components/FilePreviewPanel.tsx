import {
  FilePreviewTabs,
  type FilePreviewKind,
  type FilePreviewTabsProps,
} from '@/components/preview/FilePreviewTabs';
import { useResizableHeight } from '@/hooks/use-resizable-height';

export type { FilePreviewKind };

export type FilePreviewPanelProps = FilePreviewTabsProps;

export function FilePreviewPanel(props: FilePreviewPanelProps) {
  const {
    height: previewHeight,
    isResizing: isResizingPreview,
    onResizeStart: onPreviewResizeStart,
  } = useResizableHeight({
    defaultHeight: 288,
    minHeight: 160,
    maxHeight: 600,
    storageKey: 'filePreviewHeight',
  });

  return (
    <div
      className="bg-forensics-surface shrink-0 min-h-0 flex flex-col"
      style={{ height: `${previewHeight}px` }}
    >
      <div
        className={`shrink-0 h-1 cursor-row-resize transition-colors ${
          isResizingPreview ? 'bg-forensics-info-bg' : 'hover:bg-forensics-info-bg'
        }`}
        onMouseDown={onPreviewResizeStart}
        title="拖拽调整预览区高度"
      />
      <div className="min-h-0 flex-1">
        <FilePreviewTabs {...props} />
      </div>
    </div>
  );
}
