import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import type { EmulationWorkspaceModel } from '@/features/emulation/use-emulation-workspace-model';
import {
  BootRouteRow,
  ErrorNote,
  LaunchPanelShell,
  OptionsFieldset,
  ProtectionNote,
  RecoveryIsoField,
  SourceInfoList,
  SourceSelectField,
  StartButton,
} from './launch-shared';

interface WindowsLaunchPanelProps {
  model: EmulationWorkspaceModel;
}

function WindowsBypassFieldset({ model }: WindowsLaunchPanelProps) {
  const { t } = useTranslation();
  const samInstalls = (model.preflight?.installs ?? []).filter((install) => install.samPresent);
  if (samInstalls.length === 0) return null;
  return (
    <fieldset className="space-y-2 border-b border-forensics-border py-3">
      <legend className="text-[11px] text-forensics-muted">{t('emulationPage.bypass.title')}</legend>
      <Select
        value={model.bypassPartition === undefined ? 'none' : String(model.bypassPartition)}
        onValueChange={(value) => model.selectBypassPartition(value === 'none' ? undefined : Number(value))}
      >
        <SelectTrigger variant="forensics" aria-label={t('emulationPage.bypass.partition')}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="none">{t('emulationPage.bypass.none')}</SelectItem>
          {samInstalls.map((install) => (
            <SelectItem key={install.partitionIndex} value={String(install.partitionIndex)}>
              [P{install.partitionIndex}]
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {model.bypassPartition !== undefined ? (
        <>
          <Select
            value={model.bypassRid === undefined ? 'none' : String(model.bypassRid)}
            onValueChange={(value) => model.selectBypassRid(value === 'none' ? undefined : Number(value))}
            disabled={model.bypassAccountsLoading}
          >
            <SelectTrigger variant="forensics" aria-label={t('emulationPage.bypass.account')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="none">{t('emulationPage.bypass.none')}</SelectItem>
              {model.bypassAccounts.map((account) => (
                <SelectItem key={account.rid} value={String(account.rid)}>
                  {account.username || `RID ${account.rid}`}
                  {account.disabled ? ` · ${t('emulationPage.bypass.disabled')}` : ''}
                  {account.hasPassword ? '' : ` · ${t('emulationPage.bypass.noPassword')}`}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select
            value={model.bypassAction}
            onValueChange={(value) => model.selectBypassAction(value as typeof model.bypassAction)}
          >
            <SelectTrigger variant="forensics" aria-label={t('emulationPage.bypass.action')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="clearPassword">{t('emulationPage.bypass.clearPassword')}</SelectItem>
              <SelectItem value="enableAndClearPassword">{t('emulationPage.bypass.enableAndClear')}</SelectItem>
            </SelectContent>
          </Select>
        </>
      ) : null}
    </fieldset>
  );
}

function WindowsPreflightList({ model }: WindowsLaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.preflight || model.preflight.installs.length === 0) return null;
  return (
    <div className="space-y-1.5 border-y border-forensics-border py-3">
      <div className="text-[11px] text-forensics-muted">{t('emulationPage.preflight.title')}</div>
      {model.preflight.installs.map((install) => (
        <div key={install.partitionIndex} className="flex items-center gap-2 text-[11px]">
          <span className="font-mono text-forensics-text">[P{install.partitionIndex}]</span>
          <Badge variant={install.osdataPresent ? 'secondary' : 'outline'}>
            {t(install.osdataPresent
              ? 'emulationPage.preflight.osdataPresent'
              : 'emulationPage.preflight.osdataAbsent')}
          </Badge>
          <Badge variant={install.utilmanBypassAvailable ? 'default' : 'outline'}>
            {t(install.utilmanBypassAvailable
              ? 'emulationPage.preflight.bypassAvailable'
              : 'emulationPage.preflight.bypassUnavailable')}
          </Badge>
        </div>
      ))}
      {model.osdataCleanupPartitions.length > 0 ? (
        model.preflight.installs.some((install) => install.osdataEmpty === false) ? (
          <div className="pt-1 text-[11px] text-forensics-warning-text">
            {t('emulationPage.preflight.osdataNonEmptyHint')}
          </div>
        ) : (
          <label className="flex items-center gap-2 pt-1 text-[12px] text-forensics-text-secondary">
            <Checkbox
              checked={model.cleanupOsdata}
              onCheckedChange={model.toggleCleanupOsdata}
              aria-label={t('emulationPage.preflight.cleanupOsdata')}
            />
            {t('emulationPage.preflight.cleanupOsdata')}
            <span className="font-mono text-[10px] text-forensics-muted">
              {model.osdataCleanupPartitions.map((partition) => `[P${partition}]`).join(' ')}
            </span>
          </label>
        )
      ) : null}
    </div>
  );
}

function MaintenanceToolRow({ model }: WindowsLaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.preflight) return null;
  return (
    <div className="flex items-center gap-2 border-b border-forensics-border py-3 text-[11px]">
      <Badge variant={model.preflight.maintenanceToolAvailable ? 'default' : 'secondary'}>
        {t(model.preflight.maintenanceToolAvailable
          ? 'emulationPage.maintenance.toolAvailable'
          : 'emulationPage.maintenance.toolMissing')}
      </Badge>
      {!model.preflight.maintenanceToolAvailable ? (
        <span className="text-[10px] text-forensics-muted">
          {t('emulationPage.maintenance.buildHint')}
        </span>
      ) : null}
    </div>
  );
}

export function WindowsLaunchPanel({ model }: WindowsLaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <LaunchPanelShell>
      <SourceSelectField model={model} />
      <RecoveryIsoField model={model} />
      <BootRouteRow model={model} />
      <OptionsFieldset model={model} />
      <WindowsBypassFieldset model={model} />
      <SourceInfoList model={model} />
      <WindowsPreflightList model={model} />
      <MaintenanceToolRow model={model} />
      {model.recoveryIsoPath && model.preflight?.maintenanceToolAvailable === false ? (
        <div className="border-b border-forensics-border py-3 text-[11px] text-forensics-warning-text">
          {t('emulationPage.maintenance.peWithoutTool')}
        </div>
      ) : null}
      <ProtectionNote />
      <ErrorNote model={model} />
      <StartButton model={model} />
    </LaunchPanelShell>
  );
}
