import { useCallback, useEffect, useMemo, useState } from 'react';
import { StepRail } from '@/components/wizard/StepRail';
import { useTranslation } from 'react-i18next';
import { HardDrive, Monitor, Server, Smartphone } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Label } from '@/app/components/ui/label';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import { ToggleGroup, ToggleGroupItem } from '@/app/components/ui/toggle-group';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import type { ImportDataSourceRequest, ImportTargetPlatform, LocalDisk } from '@/types/models';
import { formatBytes } from '@/lib/format-bytes';

type SourceKind = 'auto' | 'linuxEvidenceSet' | 'localDisk';
type Step = 'platform' | 'source' | 'config' | 'confirm';

const STEP_ORDER: Step[] = ['platform', 'source', 'config', 'confirm'];

const STEP_LABEL: Record<Step, string> = {
  platform: 'importDataSource.stepper.platform',
  source: 'importDataSource.stepper.source',
  config: 'importDataSource.stepper.config',
  confirm: 'importDataSource.stepper.confirm',
};

const STEP_DESCRIPTION: Record<Step, string> = {
  platform: 'importDataSource.steps.platform',
  source: 'importDataSource.steps.source',
  config: 'importDataSource.steps.config',
  confirm: 'importDataSource.steps.confirm',
};

const PLATFORM_CARDS: { value: ImportTargetPlatform; icon: typeof Monitor; subtitle: string }[] = [
  { value: 'windows', icon: Monitor, subtitle: 'NTFS / Registry / EVTX' },
  { value: 'linux', icon: Server, subtitle: 'XFS / LVM / systemd' },
  { value: 'android', icon: Smartphone, subtitle: 'Sparse / ext4 / F2FS / EROFS' },
];

export interface ImportDataSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImport: (request: ImportDataSourceRequest) => void;
  importPending: boolean;
  pickSourcePath: (filterName: string) => Promise<string | undefined>;
  pickDirectoryPath: () => Promise<string | undefined>;
  listLocalDisks?: () => Promise<LocalDisk[]>;
}

/**
 * Four explicit steps - platform, source, path/config, confirmation - with a numbered rail so the
 * investigator always sees where they are and what the run will do before it starts.
 */
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

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="isolate overflow-hidden border-forensics-line bg-forensics-paper p-0 shadow-2xl sm:max-w-3xl">
        <DialogHeader className="border-b border-forensics-line px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold text-forensics-ink">
            {t('importDataSource.title')}
          </DialogTitle>
          <DialogDescription className="text-[11px] leading-5 text-forensics-ink-3">
            {t(STEP_DESCRIPTION[step])}
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-[164px_1fr]">
          <StepRail
            ariaLabel={t('importDataSource.stepperLabel')}
            currentId={step}
            onSelect={(id) => goToStep(id as Step)}
            steps={STEP_ORDER.map((entry) => ({ id: entry, label: t(STEP_LABEL[entry]) }))}
          />

          <div className="min-h-[268px] p-5">
            {step === 'platform' ? (
              <ToggleGroup
                type="single"
                value={platform}
                onValueChange={(value) => {
                  if (!value) return;
                  const nextPlatform = value as ImportTargetPlatform;
                  setPlatform(nextPlatform);
                  if (nextPlatform !== platform) setSourceKind('auto');
                }}
                className="grid w-full grid-cols-3 gap-2"
              >
                {PLATFORM_CARDS.map((card) => {
                  const Icon = card.icon;
                  return (
                    <ToggleGroupItem
                      key={card.value}
                      value={card.value}
                      aria-label={card.value === 'windows' ? 'Windows' : card.value === 'linux' ? 'Linux' : 'Android'}
                      className="h-24 flex-col items-start justify-start gap-1 border border-forensics-line bg-forensics-paper px-3 py-3 text-left data-[state=on]:border-forensics-red data-[state=on]:bg-forensics-red-tint"
                    >
                      <span className="flex items-center gap-2 text-[13px] font-medium text-forensics-ink">
                        <Icon size={16} />
                        {card.value === 'windows' ? 'Windows' : card.value === 'linux' ? 'Linux' : 'Android'}
                      </span>
                      <span className="text-[10px] leading-4 text-forensics-ink-3">{card.subtitle}</span>
                    </ToggleGroupItem>
                  );
                })}
              </ToggleGroup>
            ) : null}

            {step === 'source' ? (
              <div className="space-y-3">
                <div className="text-[11px] font-medium uppercase tracking-wider text-forensics-ink-3">
                  {t('importDataSource.sourceTitle')}
                </div>
                <ToggleGroup
                  type="single"
                  value={sourceKind}
                  onValueChange={(value) => {
                    if (!value) return;
                    setSourceKind(value as SourceKind);
                    setError('');
                  }}
                  className="grid w-full gap-2"
                >
                  {sourceOptions.map((option) => (
                    <ToggleGroupItem
                      key={option}
                      value={option}
                      aria-label={t(`importDataSource.modes.${option === 'auto' ? 'single' : option}`)}
                      className="h-auto flex-col items-start justify-start gap-1 border border-forensics-line bg-forensics-paper px-3 py-2 text-left data-[state=on]:border-forensics-red data-[state=on]:bg-forensics-red-tint"
                    >
                      <span className="text-[12px] text-forensics-ink">
                        {t(`importDataSource.modes.${option === 'auto' ? 'single' : option}`)}
                      </span>
                      <span className="text-[10px] leading-4 text-forensics-ink-3">
                        {t(`importDataSource.hints.${option === 'auto' ? 'single' : option}`)}
                      </span>
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              </div>
            ) : null}

            {step === 'config' ? (
              <div className="space-y-4">
                <div className="grid gap-1.5">
                  <Label htmlFor="ds-name" className="text-[11px] uppercase tracking-wider text-forensics-ink-3">
                    {t('importDataSource.fields.name')}
                  </Label>
                  <Input
                    id="ds-name"
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    placeholder={t('importDataSource.placeholders.name')}
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="ds-path" className="text-[11px] uppercase tracking-wider text-forensics-ink-3">
                    {sourceKind === 'localDisk'
                      ? t('importDataSource.fields.localDisk')
                      : t('importDataSource.fields.path')}
                  </Label>
                  <div className="flex items-center gap-2">
                    {sourceKind === 'localDisk' ? (
                      <Select value={path} onValueChange={setPath}>
                        <SelectTrigger id="ds-path" aria-label={t('importDataSource.fields.localDisk')}>
                          <SelectValue placeholder={disksLoading ? t('importDataSource.buttons.importing') : t('importDataSource.placeholders.localDisk')} />
                        </SelectTrigger>
                        <SelectContent>
                          {disks.map((disk) => (
                            <SelectItem key={disk.path} value={disk.path}>
                              {`PhysicalDrive${disk.diskNumber} · ${formatBytes(disk.size)}`}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : (
                      <Input
                        id="ds-path"
                        value={path}
                        onChange={(event) => setPath(event.target.value)}
                        placeholder={t('importDataSource.placeholders.path')}
                      />
                    )}
                    <Tooltip content={t('importDataSource.buttons.file')}>
                      <Button type="button" variant="forensicsOutline" aria-label={t('importDataSource.buttons.file')} onClick={() => void pickFile()}>
                        <ActionIcon name="file" />
                      </Button>
                    </Tooltip>
                    <Tooltip content={t('importDataSource.buttons.directory')}>
                      <Button type="button" variant="forensicsOutline" aria-label={t('importDataSource.buttons.directory')} onClick={() => void pickDirectory()}>
                        <ActionIcon name="folder" />
                      </Button>
                    </Tooltip>
                    {platform === 'linux' ? (
                      <Tooltip content={t('importDataSource.buttons.clusterDirectory')}>
                        <Button
                          type="button"
                          variant="forensicsOutline"
                          aria-label={t('importDataSource.buttons.clusterDirectory')}
                          onClick={() => void pickLinuxEvidenceSetDirectory()}
                        >
                          <ActionIcon name="hard-drive" />
                        </Button>
                      </Tooltip>
                    ) : null}
                  </div>
                </div>
                <p className="text-[11px] leading-5 text-forensics-ink-3">
                  {t(`importDataSource.hints.${sourceKind === 'auto' ? 'single' : sourceKind}`)}
                </p>
                {error ? <p className="text-[11px] text-forensics-red-ink">{error}</p> : null}
              </div>
            ) : null}

            {step === 'confirm' ? (
              <div className="space-y-3">
                <div className="text-[11px] font-medium uppercase tracking-wider text-forensics-ink-3">
                  {t('importDataSource.confirmTitle')}
                </div>
                <dl className="divide-y divide-forensics-divider border-y border-forensics-divider">
                  {confirmRows.map(([label, value]) => (
                    <div key={label} className="grid grid-cols-[132px_1fr] gap-3 py-2">
                      <dt className="text-[11px] text-forensics-ink-3">{label}</dt>
                      <dd className="min-w-0 break-all font-mono text-[11px] text-forensics-ink">{value}</dd>
                    </div>
                  ))}
                </dl>
                {error ? <p className="text-[11px] text-forensics-red-ink">{error}</p> : null}
              </div>
            ) : null}
          </div>
        </div>

        <DialogFooter className="flex items-center justify-between gap-2 border-t border-forensics-line px-5 py-3">
          <span className="text-[11px] text-forensics-ink-3">{path.trim() || t('importDataSource.confirmValues.notFilled')}</span>
          <div className="flex items-center gap-2">
            {step !== 'platform' ? (
              <Tooltip content={t('importDataSource.buttons.back')}>
                <Button type="button" variant="forensicsGhost" aria-label={t('importDataSource.buttons.back')} onClick={goBack}>
                  <ActionIcon name="chevron-left" />
                </Button>
              </Tooltip>
            ) : null}
            {step === 'confirm' ? (
              <Tooltip content={importing ? t('importDataSource.buttons.importing') : t('importDataSource.buttons.import')}>
                <Button
                  type="button"
                  variant="forensicsPrimary"
                  aria-label={importing ? t('importDataSource.buttons.importing') : t('importDataSource.buttons.import')}
                  disabled={importing}
                  onClick={handleImport}
                >
                  <ActionIcon name="upload" />
                </Button>
              </Tooltip>
            ) : (
              <Tooltip content={t('importDataSource.buttons.next')}>
                <Button type="button" variant="forensicsPrimary" aria-label={t('importDataSource.buttons.next')} onClick={goNext}>
                  <ActionIcon name="chevron-right" />
                </Button>
              </Tooltip>
            )}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
