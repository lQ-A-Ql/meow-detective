import { CircleCheck } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import { KeyValueField } from '@/components/data-display/KeyValueField';
import type { FileExtractionModel } from '@/features/files/hooks/use-file-extraction';
import { formatBytes } from '@/lib/format-bytes';

export interface FileExtractionResultDialogProps {
  model: FileExtractionModel;
}

export function FileExtractionResultDialog({ model }: FileExtractionResultDialogProps) {
  return (
    <Dialog open={model.resultOpen} onOpenChange={model.setResultOpen}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <CircleCheck className="text-forensics-success-text" />
            文件提取完成
          </DialogTitle>
          <DialogDescription>{model.result?.destinationFileName}</DialogDescription>
        </DialogHeader>

        {model.result ? (
          <div className="grid gap-2 text-[11px] sm:grid-cols-2">
            <KeyValueField label="提取大小" value={formatBytes(model.result.bytesWritten)} />
            <KeyValueField
              label="大小校验"
              value={model.result.sizeVerified ? '通过' : '未通过'}
            />
            <KeyValueField
              className="sm:col-span-2"
              label="SHA-256"
              value={model.result.sha256}
              valueClassName="break-all"
              mono
            />
          </div>
        ) : null}

        <DialogFooter>
          <Button type="button" variant="forensicsPrimary" onClick={() => model.setResultOpen(false)}>
            确定
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
