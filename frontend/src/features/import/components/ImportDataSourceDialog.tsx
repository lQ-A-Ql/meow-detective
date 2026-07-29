import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Monitor, Server, FolderOpen, Loader2 } from 'lucide-react';
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
import type { ImportDataSourceRequest, ImportTargetPlatform } from '@/types/models';
import { useImportDataSourceDialogModel } from '@/features/import/use-import-data-source-dialog-model';
import { BrandWatermark } from '@/components/brand';

type SourceKind = 'auto' | 'linuxCluster';

export interface ImportDataSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImport: (request: ImportDataSourceRequest) => void;
  importPending: boolean;
}

export function ImportDataSourceDialog({
  open,
  onOpenChange,
  onImport,
  importPending,
}: ImportDataSourceDialogProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<'platform' | 'form'>('platform');
  const [platform, setPlatform] = useState<ImportTargetPlatform>('windows');
  const [sourceKind, setSourceKind] = useState<SourceKind>('auto');
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const { pickDirectoryPath, pickSourcePath } = useImportDataSourceDialogModel();

  const reset = useCallback(() => {
    setStep('platform');
    setPlatform('windows');
    setSourceKind('auto');
    setName('');
    setPath('');
    setError('');
  }, []);

  useEffect(() => {
    if (!open) {
      reset();
    }
  }, [open, reset]);

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) reset();
    onOpenChange(nextOpen);
  }

  function goNext() {
    if (step === 'platform') {
      if (platform !== 'linux') {
        setSourceKind('auto');
      }
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
    if (sourceKind === 'linuxCluster') {
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
                if (nextPlatform !== 'linux') {
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

            <div className="space-y-2">
              <Label htmlFor="ds-path">{t('importDataSource.fields.path')}</Label>
              <div className="flex gap-2">
                <Input
                  id="ds-path"
                  value={path}
                  onChange={(e) => setPath(e.target.value)}
                  placeholder={t('importDataSource.placeholders.path')}
                  className="flex-1"
                />
                <Button variant="outline" size="sm" onClick={pickFile} className="shrink-0 gap-1">
                  <FolderOpen size={12} />
                  {t('importDataSource.buttons.file')}
                </Button>
                <Button variant="outline" size="sm" onClick={pickDirectory} className="shrink-0 gap-1">
                  <FolderOpen size={12} />
                  {t('importDataSource.buttons.directory')}
                </Button>
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
