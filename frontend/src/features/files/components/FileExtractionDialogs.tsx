import { FileExtractionDialog } from '@/features/files/components/FileExtractionDialog';
import { FileExtractionResultDialog } from '@/features/files/components/FileExtractionResultDialog';
import type { FileExtractionModel } from '@/features/files/hooks/use-file-extraction';

export interface FileExtractionDialogsProps {
  model: FileExtractionModel;
}

export function FileExtractionDialogs({ model }: FileExtractionDialogsProps) {
  return (
    <>
      <FileExtractionDialog model={model} />
      <FileExtractionResultDialog model={model} />
    </>
  );
}
