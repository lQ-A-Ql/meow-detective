import { FileClassificationBoard } from '@/features/analysis/components/panels/FileClassificationBoard';
import { ClassificationFilePreviewDialog } from '@/features/analysis/components/panels/ClassificationFilePreviewDialog';
import { useFileClassificationBoardModel } from '@/features/analysis/use-file-classification-board-model';
import type { FileClassificationBoard as FileClassificationBoardData } from '@/types/models';

export function FileClassificationBoardContainer({
  board,
}: {
  board?: FileClassificationBoardData;
}) {
  const model = useFileClassificationBoardModel();
  return (
    <>
      <FileClassificationBoard
        board={board}
        selectedFileId={model.selectedFileId}
        onSelect={model.selectRow}
      />
      <ClassificationFilePreviewDialog model={model} />
    </>
  );
}
