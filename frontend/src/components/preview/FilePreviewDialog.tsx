import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import {
  FilePreviewTabs,
  type FilePreviewTabsProps,
} from '@/components/preview/FilePreviewTabs';

export interface FilePreviewDialogProps extends FilePreviewTabsProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Dialog-based file preview composed from the shared viewer primitives.
 * Presentational only: callers wire preview data hooks and pass the results
 * through props, keeping this primitive free of feature-store coupling.
 */
export function FilePreviewDialog({
  open,
  onOpenChange,
  selectedFile,
  ...previewProps
}: FilePreviewDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[80vh] w-[calc(100vw-4rem)] max-w-[calc(100vw-4rem)] flex-col overflow-hidden sm:max-w-6xl">
        <DialogHeader>
          <DialogTitle className="truncate font-mono text-[13px]">
            {selectedFile?.name ?? '文件预览'}
          </DialogTitle>
          <DialogDescription className="truncate font-mono text-[11px]">
            {selectedFile?.path ?? ''}
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-hidden">
          <FilePreviewTabs {...previewProps} selectedFile={selectedFile} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
