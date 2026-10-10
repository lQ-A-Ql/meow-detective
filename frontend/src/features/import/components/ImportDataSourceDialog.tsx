import { useCallback, useEffect, useState } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
import { Monitor, Server, Smartphone, FolderOpen, HardDrive, Loader2, ChevronRight, ChevronLeft } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Label } from '@/app/components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import {
  ToggleGroup,
  ToggleGroupItem,
} from '@/app/components/ui/toggle-group';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import type { ImportDataSourceRequest, ImportTargetPlatform, LocalDisk } from '@/types/models';
import { BrandWatermark } from '@/components/brand';

type SourceKind = 'auto' | 'linuxEvidenceSet' | 'localDisk';

export interface ImportDataSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImport: (request: ImportDataSourceRequest) => void;
  importPending: boolean;
  pickSourcePath: (filterName: string) => Promise<string | undefined>;
  pickDirectoryPath: () => Promise<string | undefined>;
  listLocalDisks?: () => Promise<LocalDisk[]>;
}

export function ImportDataSourceDialog({
  open,
  onOpenChange,
  onImport,
  importPending,
  pickSourcePath,
  pickDirectoryPath,
  listLocalDisks,
}: ImportDataSourceDialogProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<'platform' | 'form'>('platform');
  const [platform, setPlatform] = useState<ImportTargetPlatform>('windows');
  const [sourceKind, setSourceKind] = useState<SourceKind>('auto');
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const [disks, setDisks] = useState<LocalDisk[]>([]);
  const [disksLoading, setDisksLoading] = useState(false);
  const reset = useCallback(() => {
    setStep('platform');
    setPlatform('windows');
    setSourceKind('auto');
    setName('');
    setPath('');
    setError('');
    setDisks([]);
    setDisksLoading(false);
  }, []);

  useEffect(() => {
    if (!open) {
      reset();
    }
  }, [open, reset]);

  useEffect(() => {
    if (!open || platform !== 'windows' || sourceKind !== 'localDisk' || !listLocalDisks) {
      return;
    }
    let active = true;
    setDisksLoading(true);
    void listLocalDisks()
      .then((items) => {
        if (!active) return;
        setDisks(items);
        if (items[0]) setPath((current) => current || items[0].path);
      })
      .finally(() => {
        if (active) setDisksLoading(false);
      });
    return () => {
      active = false;
    };
  }, [listLocalDisks, open, platform, sourceKind]);

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) reset();
    onOpenChange(nextOpen);
  }

  function goNext() {
    if (step === 'platform') {
      setStep('form');
      setError('');
    }
  }

  function goBack() {
    if (step === 'form') {
      setStep('platform');
      setError('');
    }
  }

  async function pickFile() {
    const selectedPath = await pickSourcePath(t('importDataSource.dialogFilters.dataSources'));
    if (selectedPath) setPath(selectedPath);
  }

  async function pickDirectory() {
    const selectedPath = await pickDirectoryPath();
    if (selectedPath) setPath(selectedPath);
  }

  async function pickLinuxEvidenceSetDirectory() {
    const selectedPath = await pickDirectoryPath();
    if (selectedPath) {
      setSourceKind('linuxEvidenceSet');
      setPath(selectedPath);
    }
  }

  function handleImport() {
    const trimmedPath = path.trim();
    if (!trimmedPath) {
      setError(t('importDataSource.errors.pathRequired'));
      return;
    }
    setError('');
    const request: ImportDataSourceRequest = {
      sourcePath: trimmedPath,
      platform,
      profile: name.trim() || undefined,
    };
    if (sourceKind !== 'auto') {
      request.sourceKind = sourceKind;
    }
    onImport(request);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="isolate overflow-hidden border-forensics-border bg-white p-0 shadow-[0_18px_60px_rgba(116,22,28,0.16)] sm:max-w-2xl">
        <BrandWatermark
          motif="walking"
          className="pointer-events-none absolute -right-7 top-1 h-24 opacity-[0.055]"
        />
        <div className="border-b border-forensics-border bg-forensics-panel px-6 py-5">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 text-[17px] font-semibold text-forensics-text">
              <span className="flex size-7 items-center justify-center bg-forensics-primary-blue text-[12px] font-semibold text-white">01</span>
              {t('importDataSource.title')}
            </DialogTitle>
            <DialogDescription className="max-w-xl text-[12px] leading-5 text-forensics-muted">
              {step === 'platform'
                ? t('importDataSource.steps.platform')
                : t('importDataSource.steps.form')}
            </DialogDescription>
          </DialogHeader>
          <div className="mt-5 flex items-center gap-2" aria-label={t('importDataSource.stepperLabel')}>
            <span className={`h-1 flex-1 ${step === 'platform' ? 'bg-forensics-primary-blue' : 'bg-forensics-border'}`} />
            <span className={`h-1 flex-1 ${step === 'form' ? 'bg-forensics-primary-blue' : 'bg-forensics-border'}`} />
          </div>
          <div className="mt-2 flex justify-between text-[10px] uppercase tracking-[0.16em] text-forensics-muted-light">
            <span className={step === 'platform' ? 'text-forensics-primary-blue' : undefined}>{t('importDataSource.stepper.platform')}</span>
            <span className={step === 'form' ? 'text-forensics-primary-blue' : undefined}>{t('importDataSource.stepper.source')}</span>
          </div>
        </div>

        {step === 'platform' ? (
          <div className="space-y-5 p-6">
            <ToggleGroup
              type="single"
              value={platform}
              onValueChange={(v) => {
                if (!v) return;
                const nextPlatform = v as ImportTargetPlatform;
                setPlatform(nextPlatform);
                if (nextPlatform !== platform) {
                  setSourceKind('auto');
                }
              }}
              className="grid w-full grid-cols-3 gap-3"
            >
              <ToggleGroupItem
                value="windows"
                aria-label="Windows"
                className="relative h-24 justify-start overflow-hidden rounded-none border border-forensics-border bg-white px-3 text-left transition-colors hover:border-forensics-primary-blue hover:bg-forensics-sakura-100 data-[state=on]:border-forensics-primary-blue data-[state=on]:bg-forensics-sakura-100"
              >
                <div className="relative z-10 flex flex-col items-start gap-0.5">
                  <span className="flex items-center gap-2 text-sm font-semibold text-forensics-text">
                    <Monitor size={18} className="text-forensics-primary-blue" />
                    Windows
                  </span>
                  <span className="text-[10px] leading-4 text-forensics-muted">NTFS / Registry / EVTX</span>
                </div>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="linux"
                aria-label="Linux"
                className="relative h-24 justify-start overflow-hidden rounded-none border border-forensics-border bg-white px-3 text-left transition-colors hover:border-forensics-primary-blue hover:bg-forensics-sakura-100 data-[state=on]:border-forensics-primary-blue data-[state=on]:bg-forensics-sakura-100"
              >
                <div className="relative z-10 flex flex-col items-start gap-0.5">
                  <span className="flex items-center gap-2 text-sm font-semibold text-forensics-text">
                    <Server size={18} className="text-forensics-primary-blue" />
                    Linux
                  </span>
                  <span className="text-[10px] leading-4 text-forensics-muted">XFS / LVM / systemd</span>
                </div>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="android"
                aria-label="Android"
                className="relative h-24 justify-start overflow-hidden rounded-none border border-forensics-border bg-white px-3 text-left transition-colors hover:border-forensics-primary-blue hover:bg-forensics-sakura-100 data-[state=on]:border-forensics-primary-blue data-[state=on]:bg-forensics-sakura-100"
              >
                <div className="relative z-10 flex flex-col items-start gap-0.5">
                  <span className="flex items-center gap-2 text-sm font-semibold text-forensics-text">
                    <Smartphone size={18} className="text-forensics-primary-blue" />
                    Android
                  </span>
                  <span className="text-[10px] leading-4 text-forensics-muted">Sparse / ext4 / F2FS / EROFS</span>
                </div>
              </ToggleGroupItem>
            </ToggleGroup>
            <div className="flex items-center justify-between border-t border-forensics-border pt-4">
              <span className="text-[11px] text-forensics-muted">{t('importDataSource.platformHint')}</span>
              <Tooltip content={t('importDataSource.buttons.next')}><Button aria-label={t('importDataSource.buttons.next')} onClick={goNext} size="sm" variant="forensicsPrimary"><ActionIcon name="upload" /></Button></Tooltip>
            </div>
          </div>
        ) : (
          <div className="space-y-5 p-6">
            <div className="grid gap-2">
              <Label htmlFor="ds-name" className="text-[11px] font-semibold uppercase tracking-wider text-forensics-text-secondary">{t('importDataSource.fields.name')}</Label>
              <Input
                id="ds-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('importDataSource.placeholders.name')}
              />
            </div>

            {platform === 'linux' ? (
              <div className="grid gap-2">
                <Label className="text-[11px] font-semibold uppercase tracking-wider text-forensics-text-secondary">{t('importDataSource.fields.mode')}</Label>
                <ToggleGroup
                  type="single"
                  value={sourceKind}
                  onValueChange={(v) => v && setSourceKind(v as SourceKind)}
                  className="grid grid-cols-2 gap-2"
                >
                  <ToggleGroupItem value="auto" aria-label={t('importDataSource.modes.single')}>
                    {t('importDataSource.modes.single')}
                  </ToggleGroupItem>
                  <ToggleGroupItem value="linuxEvidenceSet" aria-label={t('importDataSource.modes.linuxEvidenceSet')}>
                    {t('importDataSource.modes.linuxEvidenceSet')}
                  </ToggleGroupItem>
                </ToggleGroup>
                <p className="text-[11px] text-forensics-muted">
                  {sourceKind === 'linuxEvidenceSet'
                    ? t('importDataSource.hints.linuxEvidenceSet')
                    : t('importDataSource.hints.single')}
                </p>
              </div>
            ) : null}

            {platform === 'windows' ? (
              <div className="grid gap-2">
                <Label className="text-[11px] font-semibold uppercase tracking-wider text-forensics-text-secondary">{t('importDataSource.fields.mode')}</Label>
                <ToggleGroup
                  type="single"
                  value={sourceKind}
                  onValueChange={(v) => v && setSourceKind(v as SourceKind)}
                  className="grid grid-cols-2 gap-2"
                >
                  <ToggleGroupItem value="auto" aria-label={t('importDataSource.modes.single')}>
                    {t('importDataSource.modes.single')}
                  </ToggleGroupItem>
                  <ToggleGroupItem value="localDisk" aria-label={t('importDataSource.modes.localDisk')}>
                    <HardDrive size={14} />
                    {t('importDataSource.modes.localDisk')}
                  </ToggleGroupItem>
                </ToggleGroup>
                <p className="text-[11px] text-forensics-muted">
                  {sourceKind === 'localDisk'
                    ? t('importDataSource.hints.localDisk')
                    : t('importDataSource.hints.single')}
                </p>
              </div>
            ) : null}

            <div className="grid gap-2">
              <Label htmlFor="ds-path" className="text-[11px] font-semibold uppercase tracking-wider text-forensics-text-secondary">{t('importDataSource.fields.path')}</Label>
              {sourceKind === 'localDisk' && disks.length > 0 ? (
                <Select value={path} onValueChange={setPath} disabled={disksLoading}>
                  <SelectTrigger
                    aria-label={t('importDataSource.fields.localDisk')}
                    variant="forensics"
                    size="sm"
                  >
                    <SelectValue placeholder={t('importDataSource.fields.localDisk')} />
                  </SelectTrigger>
                  <SelectContent>
                    {disks.map((disk) => (
                      <SelectItem key={disk.path} value={disk.path}>
                        <span className="font-mono text-xs">{disk.path}</span>
                        <span className="text-[10px] text-forensics-muted">
                          {(disk.size / (1024 ** 3)).toFixed(1)} GB
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : null}
              <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_auto]">
                <Input
                  id="ds-path"
                  value={path}
                  onChange={(e) => setPath(e.target.value)}
                  placeholder={
                    sourceKind === 'localDisk'
                      ? t('importDataSource.placeholders.localDisk')
                      : t('importDataSource.placeholders.path')
                  }
                  className="flex-1"
                />
                {sourceKind !== 'localDisk' ? (
                  <>
                    <Tooltip content={t('importDataSource.buttons.file')}><Button aria-label={t('importDataSource.buttons.file')} type="button" variant="forensicsOutline" size="sm" onClick={pickFile} className="shrink-0"><ActionIcon name="upload" /></Button></Tooltip>
                    <Tooltip content={t('importDataSource.buttons.directory')}><Button aria-label={t('importDataSource.buttons.directory')} type="button" variant="forensicsOutline" size="sm" onClick={pickDirectory} className="shrink-0"><ActionIcon name="upload" /></Button></Tooltip>
                  </>
                ) : null}
                {platform === 'linux' ? (
                  <Tooltip content={t('importDataSource.buttons.clusterDirectory')}><Button aria-label={t('importDataSource.buttons.clusterDirectory')} type="button" variant="forensicsOutline" size="sm" onClick={pickLinuxEvidenceSetDirectory} className="shrink-0"><ActionIcon name="upload" /></Button></Tooltip>
                ) : null}
              </div>
              {error ? (
                <p className="text-[11px] text-forensics-error-text">{error}</p>
              ) : null}
            </div>

            <DialogFooter className="gap-2 border-t border-forensics-border pt-4">
              <Tooltip content={t('importDataSource.buttons.back')}><Button aria-label={t('importDataSource.buttons.back')} type="button" variant="forensicsGhost" size="sm" onClick={goBack}><ActionIcon name="arrow-up" /></Button></Tooltip>
              <Tooltip content={t('importDataSource.buttons.import')}><Button aria-label={t('importDataSource.buttons.import')}
                type="button"
                variant="forensicsPrimary"
                size="sm"
                onClick={handleImport}
                disabled={importPending}
              ><ActionIcon name="upload" /></Button></Tooltip>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
