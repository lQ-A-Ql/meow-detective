
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
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
  const { t } = useTranslation();
  const totalBytes = model.progress?.totalBytes;
  const copiedBytesText = totalBytes === undefined
    ? undefined
    : `${formatBytes(model.progress?.bytesWritten ?? 0)} / ${formatBytes(totalBytes)}`;
  const progressText = model.progress?.phase === 'finalizing'
    ? t('fileBrowser.extraction.copying', {
      copied: copiedBytesText ?? formatBytes(model.progress.bytesWritten),
    })
    : copiedBytesText ?? t('fileBrowser.extraction.preparing');

  return (
    <Dialog open={model.formOpen} onOpenChange={model.setFormOpen}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('fileBrowser.inspector.extract.button')}</DialogTitle>
          <DialogDescription className="break-all">
            {model.file?.name ?? t('fileBrowser.extraction.noFile')}
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
            <FieldLabel htmlFor="file-extraction-destination">{t('browser.columns.targetPath')}</FieldLabel>
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
              <Tooltip content={t('fileBrowser.extraction.browse')}><Button aria-label={t('fileBrowser.extraction.browse')}
                type="button"
                variant="forensicsOutline"
                size="sm"
                onClick={() => void model.browseDestination()}
                disabled={model.isExtracting}
              ><ActionIcon name="download" /></Button></Tooltip>
            </FieldRow>
            <FieldHint>{t('fileBrowser.extraction.noOverwriteHint')}</FieldHint>
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
                aria-label={t('fileBrowser.extraction.progressLabel')}
              />
            </div>
          ) : null}

          {model.error ? <FieldError role="alert">{model.error}</FieldError> : null}

          <DialogFooter>
            <Tooltip content={t('common.cancel')}><Button aria-label={t('common.cancel')}
              type="button"
              variant="forensicsGhost"
              onClick={() => model.setFormOpen(false)}
              disabled={model.isExtracting}
            ><ActionIcon name="close" /></Button></Tooltip>
            <Tooltip content={t('fileBrowser.extraction.start')}><Button aria-label={t('fileBrowser.extraction.start')} type="submit" variant="forensicsPrimary" disabled={model.isExtracting}><ActionIcon name="download" /></Button></Tooltip>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
