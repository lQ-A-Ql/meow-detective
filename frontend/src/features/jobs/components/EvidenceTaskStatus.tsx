import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Progress } from '@/app/components/ui/progress';
import type { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';

export function EvidenceTaskStatus({ model }: { model: ReturnType<typeof useEvidenceTask> }) {
  const { t } = useTranslation();
  const task = model.task;
  if (!task && !model.loading && !model.errorText) return null;
  return <div className="space-y-2 text-[11px]" aria-live="polite">
    <div>{t(`evidenceTask.states.${task?.state ?? 'queued'}`)}</div>
    {task ? <div className="break-all font-mono">{t(task.kind === 'registrySearch' ? 'evidenceTask.scanned' : task.kind === 'contentIndex' ? 'evidenceTask.entries' : 'evidenceTask.progress', { processed: task.bytesProcessed, total: task.totalBytes })}</div> : null}
    {model.loading ? <Progress aria-label={t('evidenceTask.progressLabel')} value={task?.totalBytes ? task.bytesProcessed / task.totalBytes * 100 : 0} indeterminate={!task?.totalBytes} /> : null}
    {model.errorText ? <div role="alert" className="text-forensics-error-text">{model.errorText}</div> : null}
    {task?.state === 'failed' ? <div role="alert" className="text-forensics-error-text">{t('evidenceTask.failed', { code: task.errorCode ?? '' })}</div> : null}
    <div className="flex gap-2">
      {model.loading ? <Button shape="row" variant="forensicsOutline" disabled={model.cancelPending} onClick={model.cancel}>{t('common.cancel')}</Button> : null}
      {task?.state === 'failed' || task?.state === 'cancelled' || model.errorText ? <Button shape="row" variant="forensicsOutline" onClick={model.retry}>{t('common.retry')}</Button> : null}
    </div>
  </div>;
}
