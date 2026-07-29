import { KeyRound, LockKeyhole, MemoryStick, RefreshCw, ShieldCheck, Unlock } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import type { DataSourcePartition } from '@/types/models';
import type { BitLockerUnlockMethod, BitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';

interface BitLockerVolumePanelProps {
  partition: DataSourcePartition;
  model: BitLockerVolumeModel;
}

export function BitLockerVolumePanel({ partition, model }: BitLockerVolumePanelProps) {
  const { t } = useTranslation();
  const [method, setMethod] = useState<BitLockerUnlockMethod>('password');
  const [credential, setCredential] = useState('');
  const status = model.status;
  const canSubmit = Boolean(credential) && !model.unlocking && !model.memoryUnlocking;

  const submitCredential = () => {
    const submitted = credential;
    setCredential('');
    void model.unlock(method, submitted);
  };

  return (
    <section className="border border-forensics-border bg-forensics-surface p-3 space-y-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 text-[11px] text-forensics-text">
            <ShieldCheck size={13} className="shrink-0 text-forensics-primary-blue" />
            <span className="truncate">{t('fileBrowser.inspector.bitlocker.title')}</span>
          </div>
          <div className="mt-1 truncate font-mono text-[10px] text-forensics-muted-light">
            {partition.name} · {t('fileBrowser.inspector.bitlocker.partitionLabel')} {partition.index}
          </div>
        </div>
        <Button
          type="button"
          variant="viewerControl"
          size="iconXs"
          onClick={() => void model.inspect()}
          disabled={model.loading || model.unlocking || model.memoryUnlocking || model.importing}
          aria-label={t('fileBrowser.inspector.bitlocker.refresh')}
          title={t('fileBrowser.inspector.bitlocker.refresh')}
        >
          <RefreshCw size={12} />
        </Button>
      </div>

      {model.loading && !status ? (
        <div className="text-[10px] text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.loading')}</div>
      ) : null}

      {status ? (
        <div className="grid grid-cols-[auto_1fr] gap-x-2 gap-y-1 font-mono text-[10px]">
          <span className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.statusLabel')}</span>
          <span className="text-forensics-text">{status.unlocked
            ? t('fileBrowser.inspector.bitlocker.stateUnlocked')
            : t('fileBrowser.inspector.bitlocker.stateLocked')}</span>
          <span className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.encryption')}</span>
          <span className="truncate text-forensics-text">{status.encryptionMethod}</span>
          <span className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.protectors')}</span>
          <span className="truncate text-forensics-text">{status.protectors.length}</span>
          <span className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.storedKey')}</span>
          <span className="text-forensics-text">{status.storedKeyAvailable
            ? t('fileBrowser.inspector.bitlocker.storedKeyAvailable')
            : t('fileBrowser.inspector.bitlocker.storedKeyMissing')}</span>
          {status.plaintextFilesystem ? (
            <>
              <span className="text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.plaintextFilesystem')}</span>
              <span className="text-forensics-text">{status.plaintextFilesystem}</span>
            </>
          ) : null}
        </div>
      ) : null}

      {status?.protectors.length ? (
        <div className="space-y-1">
          <div className="text-[10px] text-forensics-muted-light">{t('fileBrowser.inspector.bitlocker.protectorList')}</div>
          <ScrollArea className="max-h-20" viewportClassName="space-y-1">
            {status.protectors.map((protector) => (
              <div key={`${protector.code}-${protector.kind}`} className="flex items-center justify-between gap-2 font-mono text-[10px]">
                <span className="truncate text-forensics-text-secondary">{protector.label}</span>
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
        <div className="space-y-2 border-t border-forensics-border pt-2">
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
          <Button type="button" variant="forensicsPrimary" size="xs" className="w-full" onClick={submitCredential} disabled={!canSubmit}>
            <Unlock size={12} /> {model.unlocking
              ? t('fileBrowser.inspector.bitlocker.unlocking')
              : t('fileBrowser.inspector.bitlocker.unlock')}
          </Button>
        </div>
      ) : null}

      {status && !status.unlocked ? (
        <Button
          type="button"
          variant="forensicsSurface"
          size="xs"
          className="w-full"
          onClick={() => void model.unlockFromMemoryImage()}
          disabled={model.memoryUnlocking || model.unlocking || model.loading}
        >
          <MemoryStick size={12} /> {model.memoryUnlocking
            ? t('fileBrowser.inspector.bitlocker.memoryUnlocking')
            : t('fileBrowser.inspector.bitlocker.memoryUnlock')}
        </Button>
      ) : null}

      {status && !status.unlocked && status.storedKeyAvailable ? (
        <Button type="button" variant="forensicsSurface" size="xs" className="w-full" onClick={() => void model.restore()} disabled={model.loading || model.memoryUnlocking}>
          <KeyRound size={12} /> {t('fileBrowser.inspector.bitlocker.restore')}
        </Button>
      ) : null}

      {status?.unlocked ? (
        <div className="space-y-2 border-t border-forensics-border pt-2">
          <Button type="button" variant="forensicsPrimary" size="xs" className="w-full" onClick={() => void model.importCatalog()} disabled={model.importing || model.loading}>
            <ShieldCheck size={12} /> {model.importing
              ? t('fileBrowser.inspector.bitlocker.importing')
              : t('fileBrowser.inspector.bitlocker.importCatalog')}
          </Button>
          <Button type="button" variant="forensicsSurface" size="xs" className="w-full" onClick={() => void model.lock()} disabled={model.loading || model.importing}>
            <LockKeyhole size={12} /> {t('fileBrowser.inspector.bitlocker.lock')}
          </Button>
        </div>
      ) : null}

      {status?.storedKeyAvailable ? (
        <Button type="button" variant="forensicsDangerGhost" size="xs" className="w-full" onClick={() => void model.forget()} disabled={model.loading || model.unlocking || model.memoryUnlocking || model.importing}>
          {t('fileBrowser.inspector.bitlocker.forget')}
        </Button>
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
    </section>
  );
}
