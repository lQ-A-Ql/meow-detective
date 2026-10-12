import { useResizableHeight } from '@/hooks/use-resizable-height';
import { FilePreviewPanelView } from '../components/FilePreviewPanelView';
import type { FilePreviewPanelProps } from '../components/FilePreviewPanelView';

export function FilePreviewPanelController(props: FilePreviewPanelProps) {
  const resize = useResizableHeight({
    defaultHeight: 288,
    minHeight: 160,
    maxHeight: 600,
    storageKey: 'filePreviewHeight',
  });
  return <FilePreviewPanelView {...props} resize={resize} />;
}
