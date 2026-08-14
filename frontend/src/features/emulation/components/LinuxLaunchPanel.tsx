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

interface LinuxLaunchPanelProps {
  model: EmulationWorkspaceModel;
}

function LinuxBypassFieldset({ model }: LinuxLaunchPanelProps) {
  const { t } = useTranslation();
  const linuxInstalls = (model.preflight?.installs ?? []).filter((install) => install.platform === 'linux');
  if (linuxInstalls.length === 0) return null;
  return (
    <fieldset className="space-y-2 border-b border-forensics-border py-3">
      <legend className="text-[11px] text-forensics-muted">{t('emulationPage.bypass.titleLinux')}</legend>
      <Select
        value={model.bypassPartition === undefined ? 'none' : String(model.bypassPartition)}
        onValueChange={(value) => model.selectBypassPartition(value === 'none' ? undefined : Number(value))}
      >
        <SelectTrigger variant="forensics" aria-label={t('emulationPage.bypass.partition')}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="none">{t('emulationPage.bypass.none')}</SelectItem>
          {linuxInstalls.map((install) => (
            <SelectItem key={install.partitionIndex} value={String(install.partitionIndex)}>
              [P{install.partitionIndex}]
              {` · ${t('emulationPage.preflight.linuxInstall')}`}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {model.bypassPartition !== undefined ? (
        <Select
          value={model.linuxUsername === undefined ? 'none' : model.linuxUsername}
          onValueChange={(value) => model.selectLinuxUsername(value === 'none' ? undefined : value)}
          disabled={model.linuxAccountsLoading}
        >
          <SelectTrigger variant="forensics" aria-label={t('emulationPage.bypass.account')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">{t('emulationPage.bypass.none')}</SelectItem>
            {model.linuxAccounts.map((account) => (
              <SelectItem key={account.username} value={account.username}>
                {account.username}
                {account.locked ? ` · ${t('emulationPage.bypass.disabled')}` : ''}
                {account.hasPassword ? '' : ` · ${t('emulationPage.bypass.noPassword')}`}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : null}
      {model.linuxUsername ? (
        <div className="pt-1 text-[10px] leading-4 text-forensics-warning-text">
          {t('emulationPage.bypass.linuxPasswordHint', { username: model.linuxUsername })}
        </div>
      ) : null}
      {model.bypassPartition !== undefined && model.linuxAccountsError ? (
        <div className="pt-1 text-[10px] leading-4 text-forensics-warning-text">
          {model.linuxAccountsError}
        </div>
      ) : null}
      {model.bypassPartition !== undefined
        && !model.linuxAccountsError
        && !model.linuxAccountsLoading
        && model.linuxAccounts.length === 0 ? (
        <div className="pt-1 text-[10px] leading-4 text-forensics-muted">
          {t('emulationPage.bypass.noAccounts')}
        </div>
      ) : null}
      {model.bypassPartition !== undefined
        && !model.linuxAccountsError
        && !model.linuxAccountsLoading
        && model.linuxAccounts.length > 0
        && !model.linuxUsername ? (
        <div className="pt-1 text-[10px] leading-4 text-forensics-warning-text">
          {t('emulationPage.bypass.selectLinuxAccount')}
        </div>
      ) : null}
    </fieldset>
  );
}

function LinuxPreflightList({ model }: LinuxLaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.preflight || model.preflight.installs.length === 0) return null;
  const hasDirtyXfsLog = model.preflight.installs
    .some((install) => (install.bootRiskNotes ?? []).includes('xfs-log-dirty'));
  const hasUnverifiedXfsLog = model.preflight.installs
    .some((install) => (install.bootRiskNotes ?? []).includes('xfs-log-unverified'));
  return (
    <div className="space-y-1.5 border-y border-forensics-border py-3">
      <div className="text-[11px] text-forensics-muted">{t('emulationPage.preflight.title')}</div>
      {model.preflight.installs.map((install) => (
        <div key={install.partitionIndex} className="flex items-center gap-2 text-[11px]">
          <span className="font-mono text-forensics-text">[P{install.partitionIndex}]</span>
          <Badge variant="outline">{t('emulationPage.preflight.linuxInstall')}</Badge>
          {install.osReleasePrettyName ? (
            <span className="truncate text-forensics-text-secondary">{install.osReleasePrettyName}</span>
          ) : null}
          {install.kernelPresent === false ? (
            <Badge variant="secondary">{t('emulationPage.preflight.noKernel')}</Badge>
          ) : null}
          {install.fstabPresent === false ? (
            <Badge variant="secondary">{t('emulationPage.preflight.noFstab')}</Badge>
          ) : null}
          {(install.bootRiskNotes ?? []).includes('btrfs-root') ? (
            <Badge variant="secondary">{t('emulationPage.preflight.btrfsRoot')}</Badge>
          ) : null}
          {(install.bootRiskNotes ?? []).includes('no-efi-fallback') ? (
            <Badge variant="secondary">{t('emulationPage.preflight.noEfiFallback')}</Badge>
          ) : null}
          {(install.bootRiskNotes ?? []).includes('xfs-log-dirty') ? (
            <Badge variant="secondary">{t('emulationPage.preflight.xfsLogDirty')}</Badge>
          ) : null}
          {(install.bootRiskNotes ?? []).includes('xfs-log-unverified') ? (
            <Badge variant="secondary">{t('emulationPage.preflight.xfsLogUnverified')}</Badge>
          ) : null}
        </div>
      ))}
      {hasDirtyXfsLog ? (
        <div className="pt-1 text-[10px] leading-4 text-forensics-warning-text">
          {t('emulationPage.preflight.xfsLogDirtyHint')}
        </div>
      ) : null}
      {hasUnverifiedXfsLog ? (
        <div className="pt-1 text-[10px] leading-4 text-forensics-warning-text">
          {t('emulationPage.preflight.xfsLogUnverifiedHint')}
        </div>
      ) : null}
      <div className="pt-1 text-[10px] leading-4 text-forensics-muted">
        {t('emulationPage.preflight.linuxBypassHint')}
      </div>
    </div>
  );
}

function EfiFallbackRow({ model }: LinuxLaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.needsEfiFallback) return null;
  return (
    <div className="border-b border-forensics-border py-3">
      <label className="flex items-center gap-2 text-[12px] text-forensics-text-secondary">
        <Checkbox
          checked={model.installEfiFallback}
          onCheckedChange={model.toggleInstallEfiFallback}
          aria-label={t('emulationPage.launch.installEfiFallback')}
        />
        {t('emulationPage.launch.installEfiFallback')}
      </label>
      <div className="mt-1 pl-6 text-[10px] leading-4 text-forensics-muted">
        {t('emulationPage.launch.installEfiFallbackHint')}
      </div>
    </div>
  );
}

function FsRepairRow({ model }: LinuxLaunchPanelProps) {
  const { t } = useTranslation();
  if (!model.needsFsRepair) return null;
  return (
    <div className="border-b border-forensics-border py-3">
      <label className="flex items-center gap-2 text-[12px] text-forensics-text-secondary">
        <Checkbox
          checked={model.repairFilesystems}
          onCheckedChange={model.toggleRepairFilesystems}
          aria-label={t('emulationPage.launch.repairFilesystems')}
        />
        {t('emulationPage.launch.repairFilesystems')}
      </label>
      <div className="mt-1 pl-6 text-[10px] leading-4 text-forensics-muted">
        {t('emulationPage.launch.repairFilesystemsHint')}
      </div>
    </div>
  );
}

export function LinuxLaunchPanel({ model }: LinuxLaunchPanelProps) {
  const { t } = useTranslation();
  return (
    <LaunchPanelShell>
      <SourceSelectField model={model} />
      <RecoveryIsoField model={model} hint={t('emulationPage.launch.liveIsoHint')} />
      <BootRouteRow model={model} />
      <OptionsFieldset model={model} />
      <LinuxBypassFieldset model={model} />
      <SourceInfoList model={model} />
      <LinuxPreflightList model={model} />
      <FsRepairRow model={model} />
      <EfiFallbackRow model={model} />
      <ProtectionNote />
      <ErrorNote model={model} />
      <StartButton model={model} />
    </LaunchPanelShell>
  );
}
