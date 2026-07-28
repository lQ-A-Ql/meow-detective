import { FolderOpen, LoaderCircle } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import { Field, FieldError, FieldHint, FieldLabel, FieldRow } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import { Progress } from '@/app/components/ui/progress';
import type { FileExtractionModel } from '@/features/files/hooks/use-file-extraction';
import { formatBytes } from '@/lib/format-bytes';

export interface FileExtractionDialogProps {
  model: FileExtractionModel;
}

export function FileExtractionDialog({ model }: FileExtractionDialogProps) {
  const totalBytes = model.progress?.totalBytes;
  const progressText = totalBytes === undefined
    ? '正在准备证据读取...'
    : `${formatBytes(model.progress?.bytesWritten ?? 0)} / ${formatBytes(totalBytes)}`;

  return (
    <Dialog open={model.formOpen} onOpenChange={model.setFormOpen}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>提取文件</DialogTitle>
          <DialogDescription className="break-all">
            {model.file?.name ?? '未选择文件'}
          </DialogDescription>
        </DialogHeader>

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            void model.submit();
          }}
        >
          <Field>
            <FieldLabel htmlFor="file-extraction-destination">目标路径</FieldLabel>
            <FieldRow>
              <Input
                id="file-extraction-destination"
                variant="path"
                value={model.destinationPath}
                onChange={(event) => model.setDestinationPath(event.target.value)}
                aria-invalid={Boolean(model.validationError)}
                disabled={model.isExtracting}
                autoComplete="off"
              />
              <Button
                type="button"
                variant="forensicsOutline"
                size="sm"
                onClick={() => void model.browseDestination()}
                disabled={model.isExtracting}
              >
                <FolderOpen />
                浏览
              </Button>
            </FieldRow>
            <FieldHint>已存在的目标文件不会被覆盖。</FieldHint>
            {model.validationError ? <FieldError>{model.validationError}</FieldError> : null}
          </Field>

          {model.isExtracting || model.progress ? (
            <div className="space-y-2 border border-forensics-border bg-forensics-panel p-3">
              <div className="flex items-center justify-between gap-3 text-[11px] text-forensics-muted">
                <span>{progressText}</span>
                <span className="shrink-0 font-mono">
                  {model.progress?.percent === undefined ? '--' : `${model.progress.percent}%`}
                </span>
              </div>
              <Progress
                value={model.progress?.percent ?? 0}
                indeterminate={model.progress?.percent === undefined}
                aria-label="文件提取进度"
              />
            </div>
          ) : null}

          {model.error ? <FieldError role="alert">{model.error}</FieldError> : null}

          <DialogFooter>
            <Button
              type="button"
              variant="forensicsGhost"
              onClick={() => model.setFormOpen(false)}
              disabled={model.isExtracting}
            >
              取消
            </Button>
            <Button type="submit" variant="forensicsPrimary" disabled={model.isExtracting}>
              {model.isExtracting ? <LoaderCircle className="animate-spin" /> : null}
              {model.isExtracting ? '正在提取' : '开始提取'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
