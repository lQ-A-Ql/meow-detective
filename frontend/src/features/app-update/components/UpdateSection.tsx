import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { ConfirmationDialog } from '@/app/components/ui/confirmation-dialog';
import { Progress } from '@/app/components/ui/progress';
import { UPDATE_RELEASES_URL } from '@/design/app-update';
import type { useUpdateModel } from '../use-update-model';

export function UpdateSection({ model }: { model: ReturnType<typeof useUpdateModel> }) {
  const { t } = useTranslation();
  const { info, phase, pending, progress } = model;
  const available = info?.available;
  const canInstall = available?.schemaCompatible && info?.portable && info.configured;
  const total = progress?.totalBytes;
  return (
    <section className="space-y-3">
      <h2 className="text-[14px] text-forensics-ink">{t('appUpdate.title')}</h2>
      <p className="text-[12px] text-forensics-muted">{t('appUpdate.current', { version: info?.currentVersion ?? t('appUpdate.unknown') })}</p>
      <p className="text-[12px] text-forensics-muted">{t('appUpdate.policy')}</p>
      {info && !info.configured && <p>{t('appUpdate.unconfigured')}</p>}
      {info && !info.portable && <p>{t('appUpdate.manualFirst')}</p>}
      {available && <div className="space-y-2">
        <p>{t('appUpdate.available', { version: available.version })}</p>
        <pre className="whitespace-pre-wrap break-words text-[12px]">{available.notes || t('appUpdate.noNotes')}</pre>
        {!available.schemaCompatible && <p>{t('appUpdate.errors.schema')}</p>}
      </div>}
      <p role="status" className="text-[12px]">{model.errorKey ? t(model.errorKey) : t(`appUpdate.phases.${phase}`)}</p>
      {['downloading', 'verifying', 'restarting'].includes(phase) && <div className="max-w-xl space-y-1">
        <Progress aria-label={t('appUpdate.progress')} value={total ? Math.min(100, (progress?.downloadedBytes ?? 0) / total * 100) : undefined} indeterminate={!total} />
        {progress && <p className="font-mono text-[12px]">{t('appUpdate.bytes', { bytes: progress.downloadedBytes.toLocaleString() })}</p>}
      </div>}
      <div className="flex items-center gap-3">
        <IconButton label={t('appUpdate.check')} disabled={pending || info?.configured === false} onClick={() => void model.check()}><ActionIcon name="refresh" /></IconButton>
        {available && <IconButton label={t('appUpdate.install')} disabled={!canInstall || pending} onClick={() => model.setConfirming(true)}><ActionIcon name="download" /></IconButton>}
        <IconButton label={t('appUpdate.copyLink')} onClick={() => void model.copyDownloadLink()}><ActionIcon name="copy" /></IconButton>
        <span className="text-[12px] text-forensics-muted">{model.copied ? t('appUpdate.copied') : t('appUpdate.manual')}</span>
      </div>
      <p className="break-all font-mono text-[12px] text-forensics-muted">{UPDATE_RELEASES_URL}</p>
      <ConfirmationDialog open={model.confirming} onOpenChange={model.setConfirming} title={t('appUpdate.confirmTitle')} description={t('appUpdate.confirmDescription')} cancelLabel={t('appUpdate.later')} confirmLabel={t('appUpdate.install')} onConfirm={model.confirmInstall} pending={pending} />
    </section>
  );
}
