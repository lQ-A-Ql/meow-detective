import { StepRail } from '@/components/wizard/StepRail';
import { useTranslation } from 'react-i18next';
import { Monitor, Server, Smartphone } from 'lucide-react';
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
import type { ImportTargetPlatform } from '@/types/models';
import { formatBytes } from '@/lib/format-bytes';

import type { ImportWizardModel } from '../use-import-wizard-model';
import type { Step } from '../types';
import { STEP_ORDER, STEP_LABEL, STEP_DESCRIPTION } from '../steps';

const PLATFORM_CARDS: { value: ImportTargetPlatform; icon: typeof Monitor; subtitleKey: string }[] = [
  { value: 'windows', icon: Monitor, subtitleKey: 'importDataSource.platforms.windowsSubtitle' },
  { value: 'linux', icon: Server, subtitleKey: 'importDataSource.platforms.linuxSubtitle' },
  { value: 'android', icon: Smartphone, subtitleKey: 'importDataSource.platforms.androidSubtitle' },
];

export function ImportDataSourceDialogView({ model }: { model: ImportWizardModel }) {
  const { t } = useTranslation();
  const { open, step, platform, sourceKind, name, path, error, disks, disksLoading, importing, sourceOptions, confirmRows, setName, setPath, selectPlatform, selectSource, handleOpenChange, goToStep, goNext, goBack, pickFile, pickDirectory, pickLinuxEvidenceSetDirectory, handleImport } = model;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="isolate overflow-hidden rounded-shell border-forensics-line bg-forensics-paper p-0 shadow-2xl sm:max-w-4xl">
        <DialogHeader className="border-b border-forensics-line bg-forensics-paper px-6 py-5">
          <DialogTitle className="text-[15px] font-semibold text-forensics-ink">
            {t('importDataSource.title')}
          </DialogTitle>
          <DialogDescription className="text-[11px] leading-5 text-forensics-ink-3">
            {t(STEP_DESCRIPTION[step])}
          </DialogDescription>
        </DialogHeader>

        <div className="grid min-h-[360px] grid-cols-[156px_1fr] bg-forensics-base">
          <StepRail
            ariaLabel={t('importDataSource.stepperLabel')}
            currentId={step}
            onSelect={(id) => goToStep(id as Step)}
            steps={STEP_ORDER.map((entry) => ({ id: entry, label: t(STEP_LABEL[entry]) }))}
          />

          <div className="min-w-0 bg-forensics-paper p-6">
            {step === 'platform' ? (
              <ToggleGroup
                type="single"
                value={platform}
                onValueChange={selectPlatform}
                className="grid w-full grid-cols-3 gap-2"
              >
                {PLATFORM_CARDS.map((card) => {
                  const Icon = card.icon;
                  return (
                    <ToggleGroupItem
                      key={card.value}
                      value={card.value}
                      aria-label={card.value === 'windows' ? 'Windows' : card.value === 'linux' ? 'Linux' : 'Android'}
                    className="h-28 flex-col items-start justify-start gap-2 border border-forensics-line bg-forensics-paper px-4 py-4 text-left transition-colors data-[state=on]:border-forensics-red data-[state=on]:bg-forensics-red-tint"
                    >
                      <span className="flex items-center gap-2 text-[13px] font-medium text-forensics-ink">
                        <Icon size={16} />
                        {card.value === 'windows' ? 'Windows' : card.value === 'linux' ? 'Linux' : 'Android'}
                      </span>
                      <span className="text-[10px] leading-4 text-forensics-ink-3">{t(card.subtitleKey)}</span>
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
                  onValueChange={selectSource}
                  className="grid w-full gap-2"
                >
                  {sourceOptions.map((option) => (
                    <ToggleGroupItem
                      key={option}
                      value={option}
                      aria-label={t(`importDataSource.modes.${option === 'auto' ? 'single' : option}`)}
                      className="h-auto flex-col items-start justify-start gap-1.5 border border-forensics-line bg-forensics-paper px-4 py-3 text-left transition-colors data-[state=on]:border-forensics-red data-[state=on]:bg-forensics-red-tint"
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

        <DialogFooter className="flex items-center justify-between gap-2 border-t border-forensics-line bg-forensics-base px-6 py-4">
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
