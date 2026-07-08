import { useState } from 'react';
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
import type { ImportDataSourceRequest } from '@/types/models';
import { useImportDataSourceDialogModel } from '@/features/import/use-import-data-source-dialog-model';

type Platform = 'windows' | 'linux';

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
  const [platform, setPlatform] = useState<Platform>('windows');
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const { pickDirectoryPath, pickSourcePath } = useImportDataSourceDialogModel();

  function reset() {
    setStep('platform');
    setPlatform('windows');
    setName('');
    setPath('');
    setError('');
  }

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

  function handleImport() {
    const trimmedPath = path.trim();
    if (!trimmedPath) {
      setError(t('importDataSource.errors.pathRequired'));
      return;
    }
    setError('');
    onImport({
      sourcePath: trimmedPath,
      platform,
      profile: name.trim() || undefined,
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
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
              onValueChange={(v) => v && setPlatform(v as Platform)}
              className="w-full"
            >
              <ToggleGroupItem
                value="windows"
                aria-label="Windows"
                className="flex-1 gap-2 h-16"
              >
                <Monitor size={20} />
                <span className="text-sm font-medium">Windows</span>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="linux"
                aria-label="Linux"
                className="flex-1 gap-2 h-16"
              >
                <Server size={20} />
                <span className="text-sm font-medium">Linux</span>
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
              </div>
              {error ? (
                <p className="text-[11px] text-red-600">{error}</p>
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
                    <Loader2 size={14} className="animate-spin" />
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
