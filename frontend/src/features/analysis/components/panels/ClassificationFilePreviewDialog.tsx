import { FilePreviewDialog } from '@/components/preview/FilePreviewDialog';
import type { FileClassificationBoardModel } from '@/features/analysis/use-file-classification-board-model';

export function ClassificationFilePreviewDialog({
  model,
}: {
  model: FileClassificationBoardModel;
}) {
  return (
    <FilePreviewDialog
      open={model.open}
      onOpenChange={model.onOpenChange}
      viewerTab={model.viewerTab}
      setViewerTab={model.setViewerTab}
      viewer={model.viewerTab === 'hex' ? model.viewer : undefined}
      fileHandle={model.fileHandle}
      previewKind={model.previewKind}
      onHexJumpInputChange={model.setJumpOffsetInput}
      onHexJump={model.jumpToOffset}
      onLoadNextHexRange={model.loadNextRange}
      onLoadPreviousHexRange={model.loadPreviousRange}
      textPreview={model.textPreview}
      imagePreview={model.imagePreview}
      mediaUrl={model.mediaUrl}
      documentPreview={model.documentPreview}
      selectedFile={model.selectedFile}
      previewError={model.previewError}
      onRetryPreview={model.onRetryPreview}
    />
  );
}
