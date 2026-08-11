import { CircleAlert, Disc3, FolderOpen, HardDrive, LoaderCircle, Play, ShieldCheck, X } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
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

export function LaunchPanelShell({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  return (
    <section className="border-b border-forensics-border p-5 xl:border-b-0 xl:border-r">
      <div className="mb-5 flex items-center gap-2">
        <Disc3 className="size-4 text-forensics-primary-blue" />
        <h2 className="text-[14px] text-forensics-text">{t('emulationPage.launch.title')}</h2>
      </div>
      <div className="space-y-5">{children}</div>
    </section>
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
      <div className="flex min-w-0 gap-2">
        <Input
          id="emulation-pe-iso"
          value={model.recoveryIsoPath}
          readOnly
          placeholder={t('emulationPage.launch.recoveryPlaceholder')}
          className="min-w-0 font-mono text-[11px]"
        />
        {model.recoveryIsoPath ? (
          <Button
            type="button"
            variant="forensicsGhost"
            size="icon"
            onClick={model.clearRecoveryIso}
            title={t('emulationPage.actions.clearIso')}
            aria-label={t('emulationPage.actions.clearIso')}
          >
            <X />
          </Button>
        ) : null}
        <Button
          type="button"
          variant="forensicsOutline"
          size="icon"
          onClick={() => void model.pickRecoveryIso()}
          title={t('emulationPage.actions.chooseIso')}
          aria-label={t('emulationPage.actions.chooseIso')}
        >
          <FolderOpen />
        </Button>
      </div>
      {hint ? <FieldHint>{hint}</FieldHint> : null}
    </Field>
  );
}

export function BootRouteRow({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <div className="border-y border-forensics-border py-3">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[11px] text-forensics-muted">{t('emulationPage.launch.bootRoute')}</span>
        <Badge variant={model.bootRoute === 'recoveryMedia' ? 'default' : 'secondary'}>
          {model.bootRoute === 'recoveryMedia' ? <Disc3 /> : <HardDrive />}
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
    <fieldset className="space-y-2 border-y border-forensics-border py-3">
      <legend className="text-[11px] text-forensics-muted">{t('emulationPage.launch.options')}</legend>
      <div className="grid grid-cols-2 gap-2">
        <Field>
          <FieldLabel htmlFor="emulation-cores">
            {t('emulationPage.launch.optionCores')}: {model.options.processorCount}
          </FieldLabel>
          <Slider
            id="emulation-cores"
            min={1}
            max={64}
            step={1}
            value={[model.options.processorCount]}
            onValueChange={([value]) => {
              if (value !== undefined) model.setResourceValue('processorCount', value);
            }}
            aria-label={t('emulationPage.launch.optionCores')}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="emulation-memory">
            {t('emulationPage.launch.optionMemory')}: {model.options.memoryMib}
          </FieldLabel>
          <Slider
            id="emulation-memory"
            min={512}
            max={65536}
            step={512}
            value={[Math.min(65536, Math.max(512, model.options.memoryMib))]}
            onValueChange={([value]) => {
              if (value !== undefined) model.setResourceValue('memoryMib', value);
            }}
            aria-label={t('emulationPage.launch.optionMemory')}
          />
        </Field>
      </div>
      <Field>
        <FieldLabel htmlFor="emulation-network-mode">{t('emulationPage.launch.optionNetworkMode')}</FieldLabel>
        <Select
          value={model.options.networkMode}
          onValueChange={(value) => model.selectNetworkMode(value as EmulationNetworkMode)}
        >
          <SelectTrigger id="emulation-network-mode" variant="forensics" aria-label={t('emulationPage.launch.optionNetworkMode')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {(['off', 'hostOnly', 'nat', 'bridged'] as const).map((mode) => (
              <SelectItem key={mode} value={mode}>
                {t(`emulationPage.launch.networkModes.${mode}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      {([
        ['clipboard', 'emulationPage.launch.optionClipboard'],
        ['timeSync', 'emulationPage.launch.optionTimeSync'],
      ] as const).map(([key, labelKey]) => (
        <label key={key} className="flex items-center gap-2 text-[12px] text-forensics-text-secondary">
          <Checkbox
            checked={model.options[key]}
            onCheckedChange={() => model.toggleOption(key)}
            aria-label={t(labelKey)}
          />
          {t(labelKey)}
        </label>
      ))}
    </fieldset>
  );
}

export function SourceInfoList({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.selectedSource) return null;
  return (
    <dl className="grid grid-cols-2 gap-x-4 gap-y-3 border-y border-forensics-border py-3 text-[11px]">
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
    <div className="border border-forensics-sakura-300 bg-forensics-sakura-100/20 p-3">
      <div className="flex items-center gap-2 text-[11px] text-forensics-text">
        <ShieldCheck className="size-4 text-forensics-primary-blue" />
        {t('emulationPage.protection.title')}
      </div>
      <div className="mt-1 text-[11px] leading-5 text-forensics-muted">
        {t('emulationPage.protection.detail')}
      </div>
    </div>
  );
}

export function ErrorNote({ model }: LaunchPanelProps) {
  if (!model.error) return null;
  return (
    <div className="flex items-start gap-2 border border-forensics-error-border bg-forensics-error-bg p-3 text-[11px] text-forensics-error-text">
      <CircleAlert className="mt-0.5 size-4 shrink-0" />
      <span className="min-w-0 break-words">{model.error}</span>
    </div>
  );
}

export function StartButton({ model }: LaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <Button
      type="button"
      variant="forensicsPrimary"
      className="w-full"
      onClick={() => void model.start()}
      disabled={!model.canStart}
    >
      {model.starting ? <LoaderCircle className="animate-spin" /> : <Play />}
      {model.starting ? t('emulationPage.actions.starting') : t('emulationPage.actions.start')}
    </Button>
  );
}
