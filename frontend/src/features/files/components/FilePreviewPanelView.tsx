import { useTranslation } from 'react-i18next';
import {
  FilePreviewTabs,
  type FilePreviewKind,
  type FilePreviewTabsProps,
} from '@/components/preview/FilePreviewTabs';
import type { useResizableHeight } from '@/hooks/use-resizable-height';

export type { FilePreviewKind };

export type FilePreviewPanelProps = FilePreviewTabsProps;

export function FilePreviewPanelView({ resize, ...props }: FilePreviewPanelProps & { resize: ReturnType<typeof useResizableHeight> }) {
  const { t } = useTranslation();
  const { height: previewHeight, isResizing: isResizingPreview, onResizeStart: onPreviewResizeStart } = resize;

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
        title={t('fileBrowser.preview.resizeHandle')}
      />
      <div className="min-h-0 flex-1">
        <FilePreviewTabs {...props} />
      </div>
    </div>
  );
}
