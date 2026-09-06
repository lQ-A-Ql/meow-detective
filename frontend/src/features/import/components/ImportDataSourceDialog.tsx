import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Monitor, Server, FolderOpen, HardDrive, Loader2 } from 'lucide-react';
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
import type { ImportDataSourceRequest, ImportTargetPlatform, LocalDisk } from '@/types/models';
import { BrandWatermark } from '@/components/brand';

type SourceKind = 'auto' | 'linuxCluster' | 'localDisk';

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

  async function pickLinuxClusterDirectory() {
    const selectedPath = await pickDirectoryPath();
    if (selectedPath) {
      setSourceKind('linuxCluster');
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
      <DialogContent className="isolate overflow-hidden sm:max-w-md">
        <BrandWatermark
          motif="walking"
          className="absolute -right-7 top-1 h-24 opacity-[0.045]"
        />
        <DialogHeader>
          <DialogTitle>{t('importDataSource.title')}</DialogTitle>
          <DialogDescription>
            {step === 'platform'
              ? t('importDataSource.steps.platform')
              : t('importDataSource.steps.form')}
          </DialogDescription>
        </DialogHeader>

        {step === 'platform' ? (
          <div className="py-4 space-y-4">
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
              className="w-full gap-2"
            >
              <ToggleGroupItem
                value="windows"
                aria-label="Windows"
                className="relative h-16 flex-1 justify-start overflow-hidden rounded-none border border-forensics-border bg-transparent px-3 text-left transition-colors duration-500 hover:border-forensics-sakura-500 hover:bg-forensics-hover data-[state=on]:border-forensics-sakura-500 data-[state=on]:bg-forensics-highlight"
              >
                <div className="relative z-10 flex flex-col items-start gap-0.5">
                  <span className="flex items-center gap-2 text-sm font-light text-forensics-text">
                    <Monitor size={20} />
                    Windows
                  </span>
                  <span className="text-[10px] font-light text-forensics-muted">NTFS / Registry / EVTX</span>
                </div>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="linux"
                aria-label="Linux"
                className="relative h-16 flex-1 justify-start overflow-hidden rounded-none border border-forensics-border bg-transparent px-3 text-left transition-colors duration-500 hover:border-forensics-sakura-500 hover:bg-forensics-hover data-[state=on]:border-forensics-sakura-500 data-[state=on]:bg-forensics-highlight"
              >
                <div className="relative z-10 flex flex-col items-start gap-0.5">
                  <span className="flex items-center gap-2 text-sm font-light text-forensics-text">
                    <Server size={20} />
                    Linux
                  </span>
                  <span className="text-[10px] font-light text-forensics-muted">XFS / LVM / systemd</span>
                </div>
              </ToggleGroupItem>
            </ToggleGroup>
            <div className="flex justify-end">
              <Button onClick={goNext} size="sm">
                {t('importDataSource.buttons.next')}
              </Button>
            </div>
          </div>
        ) : (
          <div className="py-4 space-y-4">
            <div className="space-y-2">
              <Label htmlFor="ds-name">{t('importDataSource.fields.name')}</Label>
              <Input
                id="ds-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('importDataSource.placeholders.name')}
              />
            </div>

            {platform === 'linux' ? (
              <div className="space-y-2">
                <Label>{t('importDataSource.fields.mode')}</Label>
                <ToggleGroup
                  type="single"
                  value={sourceKind}
                  onValueChange={(v) => v && setSourceKind(v as SourceKind)}
                  className="grid grid-cols-2 gap-2"
                >
                  <ToggleGroupItem value="auto" aria-label={t('importDataSource.modes.single')}>
                    {t('importDataSource.modes.single')}
                  </ToggleGroupItem>
                  <ToggleGroupItem value="linuxCluster" aria-label={t('importDataSource.modes.linuxCluster')}>
                    {t('importDataSource.modes.linuxCluster')}
                  </ToggleGroupItem>
                </ToggleGroup>
                <p className="text-[11px] text-forensics-muted">
                  {sourceKind === 'linuxCluster'
                    ? t('importDataSource.hints.linuxCluster')
                    : t('importDataSource.hints.single')}
                </p>
              </div>
            ) : null}

            {platform === 'windows' ? (
              <div className="space-y-2">
                <Label>{t('importDataSource.fields.mode')}</Label>
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

            <div className="space-y-2">
              <Label htmlFor="ds-path">{t('importDataSource.fields.path')}</Label>
              {sourceKind === 'localDisk' && disks.length > 0 ? (
                <select
                  aria-label={t('importDataSource.fields.localDisk')}
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                  className="h-9 w-full border border-forensics-border bg-forensics-surface px-2 text-xs text-forensics-text"
                  disabled={disksLoading}
                >
                  {disks.map((disk) => (
                    <option key={disk.path} value={disk.path}>
                      {disk.path} ({(disk.size / (1024 ** 3)).toFixed(1)} GB)
                    </option>
                  ))}
                </select>
              ) : null}
              <div className="flex gap-2">
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
                    <Button variant="outline" size="sm" onClick={pickFile} className="shrink-0 gap-1">
                      <FolderOpen size={12} />
                      {t('importDataSource.buttons.file')}
                    </Button>
                    <Button variant="outline" size="sm" onClick={pickDirectory} className="shrink-0 gap-1">
                      <FolderOpen size={12} />
                      {t('importDataSource.buttons.directory')}
                    </Button>
                  </>
                ) : null}
                {platform === 'linux' ? (
                  <Button variant="outline" size="sm" onClick={pickLinuxClusterDirectory} className="shrink-0 gap-1">
                    <FolderOpen size={12} />
                    {t('importDataSource.buttons.clusterDirectory')}
                  </Button>
                ) : null}
              </div>
              {error ? (
                <p className="text-[11px] text-forensics-error-text">{error}</p>
              ) : null}
            </div>

            <DialogFooter className="gap-2">
              <Button variant="outline" size="sm" onClick={goBack}>
                {t('importDataSource.buttons.back')}
              </Button>
              <Button
                size="sm"
                onClick={handleImport}
                disabled={importPending}
              >
                {importPending ? (
                  <>
                    <Loader2 size={14} className="opacity-70" />
                    {t('importDataSource.buttons.importing')}
                  </>
                ) : (
                  t('importDataSource.buttons.import')
                )}
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
