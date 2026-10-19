
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';

import { ConfirmationDialog } from '@/app/components/ui/confirmation-dialog';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Field, FieldHint, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { Slider } from '@/app/components/ui/slider';
import type { EmulationWorkspaceModel } from '@/features/emulation/use-emulation-workspace-model';
import { formatBytes } from '@/lib/format-bytes';
import type { EmulationNetworkMode } from '@/types/models';

interface LaunchPanelProps {
  model: EmulationWorkspaceModel;
}

export function LaunchPanelShell({ model, recoveryHint, inspection, maintenance }: LaunchPanelProps & {
  recoveryHint?: string; inspection?: ReactNode; maintenance?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid min-w-0 gap-8 pb-2 md:grid-cols-2">
      <section className="min-w-0 space-y-5">
        <h2 className="text-[14px] text-forensics-ink">{t('emulationPage.launch.title')}</h2>
        <SourceSelectField model={model} />
        <SourceInfoList model={model} />
        {model.sourceLayoutHint ? <FieldHint>{model.sourceLayoutHint}</FieldHint> : null}
        <RecoveryIsoField model={model} hint={recoveryHint} />
        <BootRouteRow model={model} />
        {inspection}
      </section>
      <section className="min-w-0 space-y-6">
        <OptionsFieldset model={model} />
        {maintenance}
      </section>
    </div>
  );
}
export function SourceSelectField({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <Field>
      <FieldLabel htmlFor="emulation-source">{t('emulationPage.launch.source')}</FieldLabel>
      <Select value={model.selectedSourceId} onValueChange={model.selectSource}>
        <SelectTrigger id="emulation-source" variant="forensics" disabled={model.sourceOptions.length === 0}>
          <SelectValue placeholder={t('emulationPage.launch.sourcePlaceholder')} />
        </SelectTrigger>
        <SelectContent>
          {model.sourceOptions.map((source) => (
            <SelectItem key={source.id} value={source.id}>
              {source.name} ({source.platform} / {source.kind})
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {model.sourceOptions.length === 0 ? (
        <FieldHint>{t('emulationPage.launch.noSources')}</FieldHint>
      ) : null}
    </Field>
  );
}

export function RecoveryIsoField({ model, hint }: LaunchPanelProps & { hint?: string }) {
  const { t } = useTranslation();
  return (
    <Field>
      <FieldLabel htmlFor="emulation-pe-iso">{t('emulationPage.launch.recoveryIso')}</FieldLabel>
      <div className="flex min-w-0 items-center gap-2">
        <Input id="emulation-pe-iso" value={model.recoveryIsoPath} readOnly
          placeholder={t('emulationPage.launch.recoveryPlaceholder')} variant="path" />
        {model.recoveryIsoPath ? <IconButton label={t('emulationPage.actions.clearIso')} onClick={model.clearRecoveryIso}>
          <ActionIcon name="close" />
        </IconButton> : null}
        <IconButton label={t('emulationPage.actions.chooseIso')} onClick={() => void model.pickRecoveryIso()}>
          <ActionIcon name="folder-open" />
        </IconButton>
      </div>
      {hint ? <FieldHint>{hint}</FieldHint> : null}
    </Field>
  );
}
export function BootRouteRow({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[11px] text-forensics-muted">{t('emulationPage.launch.bootRoute')}</span>
        <Badge variant={model.bootRoute === 'recoveryMedia' ? 'default' : 'secondary'}>
          <ActionIcon name={model.bootRoute === 'recoveryMedia' ? 'volume' : 'hard-drive'} />
          {t(`emulationPage.bootRoutes.${model.bootRoute}`)}
        </Badge>
      </div>
      {model.preflight && model.preflight.recommendedBootRoute !== model.bootRoute ? (
        <div className="mt-2 text-[10px] text-forensics-warning-text">
          {t('emulationPage.launch.recommendedRoute', {
            route: t(`emulationPage.bootRoutes.${model.preflight.recommendedBootRoute}`),
          })}
        </div>
      ) : null}
    </div>
  );
}

export function OptionsFieldset({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <fieldset className="min-w-0 space-y-5">
      <legend className="mb-5 text-[14px] text-forensics-ink">{t('emulationPage.launch.options')}</legend>
      <Field>
        <div className="flex items-center justify-between gap-3">
          <FieldLabel htmlFor="emulation-cores">{t('emulationPage.launch.optionCores')}</FieldLabel>
          <output htmlFor="emulation-cores" aria-label={t('emulationPage.launch.optionCores')} className="font-mono text-[12px] text-forensics-ink">{model.options.processorCount}</output>
        </div>
        <Slider id="emulation-cores" className="h-6" min={1} max={64} step={1} value={[model.options.processorCount]}
          onValueChange={([value]) => { if (value !== undefined) model.setResourceValue('processorCount', value); }}
          aria-label={t('emulationPage.launch.optionCores')} />
      </Field>
      <Field>
        <div className="flex items-center justify-between gap-3">
          <FieldLabel htmlFor="emulation-memory">{t('emulationPage.launch.optionMemory')}</FieldLabel>
          <output htmlFor="emulation-memory" aria-label={t('emulationPage.launch.optionMemory')} className="font-mono text-[12px] text-forensics-ink">{model.options.memoryMib}</output>
        </div>
        <Slider id="emulation-memory" className="h-6" min={512} max={65536} step={512}
          value={[Math.min(65536, Math.max(512, model.options.memoryMib))]}
          onValueChange={([value]) => { if (value !== undefined) model.setResourceValue('memoryMib', value); }}
          aria-label={t('emulationPage.launch.optionMemory')} />
      </Field>
      <Field>
        <FieldLabel htmlFor="emulation-network-mode">{t('emulationPage.launch.optionNetworkMode')}</FieldLabel>
        <Select value={model.options.networkMode} onValueChange={(value) => model.selectNetworkMode(value as EmulationNetworkMode)}>
          <SelectTrigger id="emulation-network-mode" aria-label={t('emulationPage.launch.optionNetworkMode')}><SelectValue /></SelectTrigger>
          <SelectContent>{(['off', 'hostOnly', 'nat', 'bridged'] as const).map((mode) => (
            <SelectItem key={mode} value={mode}>{t(`emulationPage.launch.networkModes.${mode}`)}</SelectItem>
          ))}</SelectContent>
        </Select>
      </Field>
      <div className="grid gap-3 sm:grid-cols-2">
        {([
          ['clipboard', 'emulationPage.launch.optionClipboard'],
          ['timeSync', 'emulationPage.launch.optionTimeSync'],
        ] as const).map(([key, labelKey]) => (
          <label key={key} className="flex items-center gap-2 text-[12px] text-forensics-text-secondary">
            <Checkbox checked={model.options[key]} onCheckedChange={() => model.toggleOption(key)} aria-label={t(labelKey)} />
            {t(labelKey)}
          </label>
        ))}
      </div>
    </fieldset>
  );
}
export function SourceInfoList({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.selectedSource) return null;
  return (
    <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-[11px] sm:grid-cols-4 md:grid-cols-2 lg:grid-cols-4">
      <div className="min-w-0">
        <dt className="text-forensics-muted">{t('emulationPage.source.kind')}</dt>
        <dd className="mt-1 font-mono text-forensics-text">{model.selectedSource.kind}</dd>
      </div>
      <div className="min-w-0">
        <dt className="text-forensics-muted">{t('emulationPage.source.platform')}</dt>
        <dd className="mt-1 font-mono text-forensics-text">{model.selectedSource.platform}</dd>
      </div>
      <div className="min-w-0">
        <dt className="text-forensics-muted">{t('emulationPage.source.partitions')}</dt>
        <dd className="mt-1 font-mono text-forensics-text">{model.selectedSource.partitionCount}</dd>
      </div>
      <div className="min-w-0">
        <dt className="text-forensics-muted">{t('emulationPage.source.evidenceSize')}</dt>
        <dd className="mt-1 font-mono text-forensics-text">
          {model.selectedSource.evidenceSize === undefined
            ? t('emulationPage.source.unknown')
            : formatBytes(model.selectedSource.evidenceSize)}
        </dd>
      </div>
    </dl>
  );
}

export function ProtectionNote() {
  const { t } = useTranslation();
  return (
    <div className="flex min-w-0 items-start gap-2">
      <ActionIcon name="shield-check" className="mt-0.5 size-4 text-forensics-ink" />
      <div className="min-w-0">
        <div className="text-[11px] text-forensics-ink">{t('emulationPage.protection.title')}</div>
        <div className="text-[11px] leading-5 text-forensics-muted">{t('emulationPage.protection.detail')}</div>
      </div>
    </div>
  );
}
export function ErrorNote({ model }: LaunchPanelProps) {
  if (!model.error) return null;
  return (
    <div role="alert" className="mt-4 flex items-start gap-2 text-[11px] text-forensics-error-text">
      <ActionIcon name="warning" className="mt-0.5 size-4" />
      <span className="min-w-0 break-words">{model.error}</span>
    </div>
  );
}
export function StartButton({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <>
      <IconButton label={t(model.starting ? 'emulationPage.actions.starting' : 'emulationPage.actions.start')}
        variant="solid" onClick={() => void model.start()} disabled={!model.canStart}>
        <ActionIcon name={model.starting ? 'refresh' : 'play'} className={model.starting ? 'size-[15px] animate-spin' : 'size-[15px]'} />
      </IconButton>
      <ConfirmationDialog open={model.directBootConfirmationOpen}
        onOpenChange={(open) => { if (!open) model.cancelDirectBoot(); }}
        title={t('emulationPage.boot.directBootTitle')} description={t('emulationPage.boot.directBootConfirm')}
        cancelLabel={t('common.cancel')} confirmLabel={t('emulationPage.boot.directBootAction')}
        onConfirm={() => void model.confirmDirectBoot()} pending={model.starting} destructive />
    </>
  );
}
