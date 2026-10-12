import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { errorMessage } from '@/lib/errors';
import { formatBytes } from '@/lib/format-bytes';
import type { ImportDataSourceRequest, ImportTargetPlatform, LocalDisk } from '@/types/models';
import type { ImportDataSourceDialogProps, SourceKind, Step } from './types';
import { STEP_ORDER } from './steps';

export function useImportWizardModel({
  open,
  onOpenChange,
  onImport,
  importPending,
  pickSourcePath,
  pickDirectoryPath,
  listLocalDisks,
}: ImportDataSourceDialogProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<Step>('platform');
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
    if (!open) reset();
  }, [open, reset]);

  useEffect(() => {
    if (!open || platform !== 'windows' || sourceKind !== 'localDisk' || !listLocalDisks) return;
    let active = true;
    setDisksLoading(true);
    void listLocalDisks()
      .then((items) => {
        if (!active) return;
        setDisks(items);
        if (items[0]) setPath((current) => current || items[0].path);
      })
      .catch((cause: unknown) => { if (active) setError(errorMessage(cause)); })
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

  function goToStep(next: Step) {
    setStep(next);
    setError('');
  }

  function goNext() {
    if (step === 'platform') return goToStep('source');
    if (step === 'source') return goToStep('config');
    if (step === 'config') {
      if (!path.trim()) {
        setError(t('importDataSource.errors.pathRequired'));
        return;
      }
      return goToStep('confirm');
    }
  }

  function goBack() {
    const index = STEP_ORDER.indexOf(step);
    if (index > 0) goToStep(STEP_ORDER[index - 1]);
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
    if (sourceKind !== 'auto') request.sourceKind = sourceKind;
    onImport(request);
  }

  const sourceOptions = useMemo(() => {
    const options: SourceKind[] = ['auto'];
    if (platform === 'linux') options.push('linuxEvidenceSet');
    if (platform === 'windows') options.push('localDisk');
    return options;
  }, [platform]);

  const sourceLabel = t(`importDataSource.modes.${sourceKind === 'auto' ? 'single' : sourceKind}`);
  const selectedDisk = sourceKind === 'localDisk' ? disks.find((disk) => disk.path === path) : undefined;
  const partitionsValue = sourceKind === 'linuxEvidenceSet'
    ? t('importDataSource.confirmValues.partitionsSet')
    : sourceKind === 'localDisk'
      ? t('importDataSource.confirmValues.partitionsDisk')
      : t('importDataSource.confirmValues.partitionsSingle');

  const confirmRows: [string, string][] = [
    [t('importDataSource.confirmRows.platform'), platform.toUpperCase()],
    [t('importDataSource.confirmRows.sourceType'), sourceLabel],
    [t('importDataSource.confirmRows.path'), path.trim() || t('importDataSource.confirmValues.notSelected')],
    [
      t('importDataSource.confirmRows.size'),
      selectedDisk ? formatBytes(selectedDisk.size) : t('importDataSource.confirmValues.sizeUnknown'),
    ],
    [t('importDataSource.confirmRows.readOnly'), t('importDataSource.confirmValues.readOnlyYes')],
    [t('importDataSource.confirmRows.partitions'), partitionsValue],
    [t('importDataSource.confirmRows.stages'), t('importDataSource.confirmValues.stages')],
  ];

  const importing = importPending;

  function selectPlatform(value: string) {
    if (value !== 'windows' && value !== 'linux' && value !== 'android') return;
    setPlatform(value);
    if (value !== platform) { setSourceKind('auto'); setPath(''); }
  }
  function selectSource(value: string) {
    if (!sourceOptions.includes(value as SourceKind)) return;
    setSourceKind(value as SourceKind);
    if (value !== sourceKind) setPath('');
    setError('');
  }
  return { open, step, platform, sourceKind, name, path, error, disks, disksLoading, importing, sourceOptions, confirmRows, setName, setPath, selectPlatform, selectSource, handleOpenChange, goToStep, goNext, goBack, pickFile, pickDirectory, pickLinuxEvidenceSetDirectory, handleImport };
}

export type ImportWizardModel = ReturnType<typeof useImportWizardModel>;
