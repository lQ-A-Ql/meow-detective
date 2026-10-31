import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import type { BitLockerPanelState } from '../hooks/use-bitlocker-panel-state';
import { useTranslation } from 'react-i18next';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { Checkbox } from '@/app/components/ui/checkbox';
import type { DataSourcePartition } from '@/types/models';
import type { BitLockerUnlockMethod, BitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';

interface BitLockerVolumePanelProps {
  partition: DataSourcePartition;
  model: BitLockerVolumeModel;
  panel: BitLockerPanelState;
}

export function BitLockerVolumePanel({ partition, model, panel }: BitLockerVolumePanelProps) {
  const { t } = useTranslation();
  const { method, setMethod, credential, setCredential, dictionaryBackend, setDictionaryBackend,
    passwordCopied, canSubmit, submitCredential, copyRecoveredPassword } = panel;
  const status = model.status;
  const reconstruction = status?.recoveryPasswordReconstruction;
  const busy = model.loading || model.unlocking || model.memoryUnlocking || model.importing || model.dictionaryAttacking;
  const statusLabel = status?.unlocked
    ? t('fileBrowser.inspector.bitlocker.stateUnlocked')
    : t('fileBrowser.inspector.bitlocker.stateLocked');

  return (
    <section className="overflow-hidden border border-forensics-border bg-forensics-surface">
      <div className="flex items-start justify-between gap-2 border-b border-forensics-border bg-forensics-panel-strong px-3 py-2.5">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 text-[11px] text-forensics-text">
            <ActionIcon name="shield-check" className="size-[14px] text-forensics-primary-blue" />
            <span className="truncate">{t('fileBrowser.inspector.bitlocker.title')}</span>
          </div>
          <div className="mt-1 truncate font-mono text-[10px] text-forensics-muted">
            {partition.name} · {t('fileBrowser.inspector.bitlocker.partitionLabel')} {partition.index}
          </div>
        </div>
        <IconButton
          label={t('fileBrowser.inspector.bitlocker.refresh')}
          variant="forensicsGhost"
          size="iconXs"
          onClick={() => void model.inspect()}
          disabled={busy}
        >
          <ActionIcon name="refresh" />
        </IconButton>
      </div>

      <div className="space-y-3 p-3">
      {model.loading && !status ? (
        <div role="status" aria-live="polite" className="text-[10px] text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.loading')}</div>
      ) : null}

      {status ? (
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 border border-forensics-border-light bg-forensics-panel px-2.5 py-2 font-mono text-[10px]">
          <dt className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.statusLabel')}</dt>
          <dd className={status.unlocked ? 'text-forensics-success' : 'text-forensics-warning-text'}>{statusLabel}</dd>
          <dt className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.encryption')}</dt>
          <dd className="truncate text-forensics-text">{status.encryptionMethod}</dd>
          <dt className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.protectors')}</dt>
          <dd className="truncate text-forensics-text">{status.protectors.length}</dd>
          <dt className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.storedKey')}</dt>
          <dd className="text-forensics-text">{status.storedKeyAvailable
            ? t('fileBrowser.inspector.bitlocker.storedKeyAvailable')
            : t('fileBrowser.inspector.bitlocker.storedKeyMissing')}</dd>
          {status.plaintextFilesystem ? (
            <>
              <dt className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.plaintextFilesystem')}</dt>
              <dd className="text-forensics-text">{status.plaintextFilesystem}</dd>
            </>
          ) : null}
        </dl>
      ) : null}

      {reconstruction ? (
        <div className="space-y-1.5 border-t border-forensics-border pt-3">
          {reconstruction.status === 'recovered' && reconstruction.password ? (
            <>
              <div className="flex items-center gap-1.5 text-[10px] text-forensics-success">
                <ActionIcon name="key" className="size-[13px]" />
                {t('fileBrowser.inspector.bitlocker.reconstructionRecovered')}
              </div>
              <div className="flex items-center gap-1.5 border border-forensics-success-border bg-forensics-success-bg px-2 py-1.5">
                <code className="min-w-0 flex-1 select-all break-all font-mono text-[10px] leading-4 text-forensics-text">
                  {reconstruction.password}
                </code>
                <IconButton
                  label={t('fileBrowser.inspector.bitlocker.copyRecoveryPassword')}
                  variant="forensicsSurface"
                  size="iconXs"
                  className="shrink-0"
                  onClick={() => copyRecoveredPassword(reconstruction.password ?? '')}
                >
                  <ActionIcon name="copy" />
                </IconButton>
              </div>
              {passwordCopied ? (
                <div className="text-[10px] text-forensics-success">
                  {t('fileBrowser.inspector.bitlocker.recoveryPasswordCopied')}
                </div>
              ) : null}
            </>
          ) : (
            <div className="text-[10px] leading-4 text-forensics-muted-light">
              {t('fileBrowser.inspector.bitlocker.reconstructionUnavailable')}
              {reconstruction.reason ? `：${reconstruction.reason}` : ''}
            </div>
          )}
        </div>
      ) : null}

      {status?.protectors.length ? (
        <div className="space-y-1.5 border-t border-forensics-border pt-3">
          <div className="text-[10px] font-medium text-forensics-muted">{t('fileBrowser.inspector.bitlocker.protectorList')}</div>
          <ScrollArea className="max-h-24 border border-forensics-border-light bg-forensics-panel px-2" viewportClassName="space-y-1.5 py-1.5">
            {status.protectors.map((protector) => (
              <div key={`${protector.code}-${protector.kind}`} className="flex items-center justify-between gap-2 font-mono text-[10px]">
                <span className="truncate text-forensics-text-secondary">
                  {t(`fileBrowser.inspector.bitlocker.protectorKinds.${protector.kind}`, { defaultValue: protector.label })}
                </span>
                <span className={protector.unlockable ? 'text-forensics-success' : 'text-forensics-muted-light'}>
                  {protector.unlockable
                    ? t('fileBrowser.inspector.bitlocker.unlockable')
                    : t('fileBrowser.inspector.bitlocker.unavailable')}
                </span>
              </div>
            ))}
          </ScrollArea>
        </div>
      ) : null}

      {status && !status.unlocked && (status.supportsPassword || status.supportsRecoveryPassword) ? (
        <div className="space-y-2 border-t border-forensics-border pt-3">
          <div className="text-[10px] font-medium text-forensics-muted">{t('fileBrowser.inspector.bitlocker.unlockSection')}</div>
          <Select value={method} onValueChange={(value) => setMethod(value as BitLockerUnlockMethod)}>
            <SelectTrigger variant="mono" size="xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {status.supportsPassword
                ? <SelectItem value="password">{t('fileBrowser.inspector.bitlocker.password')}</SelectItem>
                : null}
              {status.supportsRecoveryPassword
                ? <SelectItem value="recoveryPassword">{t('fileBrowser.inspector.bitlocker.recoveryPassword')}</SelectItem>
                : null}
            </SelectContent>
          </Select>
          <Input
            type="password"
            variant="mono"
            inputSize="compact"
            value={credential}
            onChange={(event) => setCredential(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && canSubmit) {
                event.preventDefault();
                submitCredential();
              }
            }}
            placeholder={method === 'recoveryPassword'
              ? t('fileBrowser.inspector.bitlocker.recoveryPlaceholder')
              : t('fileBrowser.inspector.bitlocker.credentialPlaceholder')}
            autoComplete="off"
            aria-label={t('fileBrowser.inspector.bitlocker.credentialLabel')}
          />
          <IconButton
            label={t('fileBrowser.inspector.bitlocker.unlock')}
            variant="forensicsPrimary"
            size="iconXs"
            onClick={submitCredential}
            disabled={!canSubmit}
          >
            <ActionIcon name="unlock" />
          </IconButton>
        </div>
      ) : null}

      {status && !status.unlocked && status.supportsPassword ? (
        <div className="space-y-2 border-t border-forensics-border pt-3">
          <div className="text-[10px] font-medium text-forensics-muted">{t('fileBrowser.inspector.bitlocker.dictionarySection')}</div>
          <label className="flex items-center gap-2 text-[10px] text-forensics-text">
            <Checkbox checked={dictionaryBackend === 'gpu'}
              disabled={model.dictionaryAttacking}
              onCheckedChange={(checked) => setDictionaryBackend(checked === true ? 'gpu' : 'cpu')} variant="forensics" />
            {t('fileBrowser.inspector.bitlocker.gpuAcceleration')}
          </label>
          {dictionaryBackend === 'gpu' ? <p className="text-[10px] text-forensics-muted-light">
            {t('fileBrowser.inspector.bitlocker.gpuRequirements')}
          </p> : null}
          <IconButton
            label={t('fileBrowser.inspector.bitlocker.importDictionary')}
            variant="forensicsSurface"
            size="iconXs"
            onClick={() => void model.startDictionaryAttack(dictionaryBackend)}
            disabled={busy}
          >
            <ActionIcon name="upload" />
          </IconButton>
        </div>
      ) : null}

      {model.dictionaryAttack ? (
        <div className="space-y-1.5 border-t border-forensics-border pt-3 font-mono text-[10px]">
          <div className="text-forensics-muted-light">{model.dictionaryAttack.backend === 'gpu'
            ? t('fileBrowser.inspector.bitlocker.gpuBackend')
            : t('fileBrowser.inspector.bitlocker.cpuBackend')}</div>
          {model.dictionaryAttack.phase === 'queued' || model.dictionaryAttack.phase === 'running' || model.dictionaryAttack.phase === 'cancelling' ? (
            <>
              <div className="text-forensics-text">
                {t('fileBrowser.inspector.bitlocker.dictionaryProgress', {
                  tested: model.dictionaryAttack.testedCandidates,
                  processed: model.dictionaryAttack.bytesProcessed,
                  total: model.dictionaryAttack.totalBytes,
                })}
              </div>
              <IconButton
                label={t('fileBrowser.inspector.bitlocker.cancelDictionary')}
                variant="forensicsDangerGhost"
                size="iconXs"
                onClick={() => void model.cancelDictionaryAttack()}
                disabled={model.dictionaryCancelling || model.dictionaryAttack.phase === 'cancelling'}
              >
                <ActionIcon name="close" />
              </IconButton>
            </>
          ) : (
            <div className={model.dictionaryAttack.phase === 'found'
              ? 'text-forensics-success'
              : 'text-forensics-muted-light'}>
              {model.dictionaryAttack.phase === 'found'
                ? t('fileBrowser.inspector.bitlocker.dictionaryFound')
                : model.dictionaryAttack.phase === 'exhausted'
                  ? t('fileBrowser.inspector.bitlocker.dictionaryExhausted')
                  : model.dictionaryAttack.phase === 'cancelled'
                    ? t('fileBrowser.inspector.bitlocker.dictionaryCancelled')
                    : model.dictionaryAttack.error === 'BITLOCKER_GPU_UNAVAILABLE'
                      ? t('fileBrowser.inspector.bitlocker.gpuUnavailable')
                      : model.dictionaryAttack.error === 'BITLOCKER_GPU_BUSY'
                        ? t('fileBrowser.inspector.bitlocker.gpuBusy')
                        : model.dictionaryAttack.error === 'BITLOCKER_GPU_FAILED'
                          ? t('fileBrowser.inspector.bitlocker.gpuFailed')
                          : t('fileBrowser.inspector.bitlocker.dictionaryFailed')}
            </div>
          )}
        </div>
      ) : null}

      {status && !status.unlocked ? (
        <div className="flex flex-wrap items-center gap-1.5 border-t border-forensics-border pt-3">
        <IconButton
          label={t('fileBrowser.inspector.bitlocker.memoryUnlock')}
          variant="forensicsSurface"
          size="iconXs"
          onClick={() => void model.unlockFromMemoryImage()}
          disabled={model.memoryUnlocking || model.unlocking || model.loading || model.dictionaryAttacking}
        >
          <ActionIcon name="memory" />
        </IconButton>
        {status.storedKeyAvailable ? (
          <IconButton
            label={t('fileBrowser.inspector.bitlocker.restore')}
            variant="forensicsSurface"
            size="iconXs"
            onClick={() => void model.restore()}
            disabled={model.loading || model.memoryUnlocking || model.dictionaryAttacking}
          >
            <ActionIcon name="restore" />
          </IconButton>
        ) : null}
        </div>
      ) : null}

      {status?.unlocked ? (
        <div className="flex flex-wrap items-center gap-1.5 border-t border-forensics-border pt-3">
          <IconButton
            label={t('fileBrowser.inspector.bitlocker.importCatalog')}
            variant="forensicsPrimary"
            size="iconXs"
            onClick={() => void model.importCatalog()}
            disabled={model.importing || model.loading}
          >
            <ActionIcon name="upload" />
          </IconButton>
          <IconButton
            label={t('fileBrowser.inspector.bitlocker.lock')}
            variant="forensicsSurface"
            size="iconXs"
            onClick={() => void model.lock()}
            disabled={model.loading || model.importing || model.dictionaryAttacking}
          >
            <ActionIcon name="lock" />
          </IconButton>
        </div>
      ) : null}

      {status?.storedKeyAvailable ? (
        <div className="flex justify-end border-t border-forensics-border pt-3">
          <IconButton
            label={t('fileBrowser.inspector.bitlocker.forget')}
            variant="forensicsDangerGhost"
            size="iconXs"
            onClick={() => void model.forget()}
            disabled={busy}
          >
            <ActionIcon name="erase" />
          </IconButton>
        </div>
      ) : null}

      {model.catalog ? (
        <div className="border-t border-forensics-border pt-2 font-mono text-[10px] text-forensics-muted-light">
          {t('fileBrowser.inspector.bitlocker.catalogSummary', {
            files: model.catalog.fileCount ?? 0,
            directories: model.catalog.directoryCount ?? 0,
          })}
        </div>
      ) : null}
      {model.error ? <div role="alert" className="text-[10px] leading-4 text-forensics-error-text">{model.error}</div> : null}
      </div>
    </section>
  );
}
