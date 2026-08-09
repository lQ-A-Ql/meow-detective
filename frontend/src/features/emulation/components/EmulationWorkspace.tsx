import {
  CircleAlert,
  Disc3,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  Play,
  Power,
  RefreshCw,
  ShieldCheck,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Field, FieldHint, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { Slider } from '@/app/components/ui/slider';
import type {
  EmulationSessionView,
  EmulationWorkspaceModel,
} from '@/features/emulation/use-emulation-workspace-model';
import { formatBytes } from '@/lib/format-bytes';
import type { EmulationNetworkMode, EmulationState } from '@/types/models';

interface EmulationWorkspaceProps {
  model: EmulationWorkspaceModel;
}

function stateVariant(state: EmulationState): 'default' | 'secondary' | 'destructive' | 'outline' {
  if (state === 'running') return 'default';
  if (state === 'failedCleanupPending') return 'destructive';
  if (state === 'released') return 'outline';
  return 'secondary';
}

function SessionRow({
  session,
  releasing,
  onRelease,
}: {
  session: EmulationSessionView;
  releasing: boolean;
  onRelease: (sessionId: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid min-w-[760px] grid-cols-[minmax(180px,1.4fr)_140px_120px_minmax(180px,1fr)_52px] items-center border-b border-forensics-border-light px-4 py-3 text-[12px] last:border-b-0 hover:bg-forensics-hover">
      <div className="min-w-0">
        <div className="truncate text-forensics-text" title={session.sourceName}>{session.sourceName}</div>
        <div className="mt-1 truncate font-mono text-[10px] text-forensics-muted" title={session.sessionId}>
          {session.sessionId}
        </div>
      </div>
      <Badge variant={stateVariant(session.state)}>
        {t(`emulationPage.states.${session.state}`)}
      </Badge>
      <span className="font-mono text-[11px] text-forensics-text-secondary">
        {formatBytes(session.logicalLength)}
      </span>
      <div className="min-w-0">
        <div className="truncate text-forensics-text-secondary">
          {t(`emulationPage.controlModes.${session.controlMode}`)}
        </div>
        <div className="mt-1 truncate text-[10px] text-forensics-muted">
          {t(session.maintenanceMedia
            ? 'emulationPage.maintenance.attached'
            : 'emulationPage.maintenance.absent')}
        </div>
        {session.error ? (
          <div className="mt-1 truncate text-[10px] text-forensics-error-text" title={session.error}>
            {session.error}
          </div>
        ) : null}
      </div>
      <Button
        type="button"
        variant="forensicsGhost"
        size="iconSm"
        onClick={() => void onRelease(session.sessionId)}
        disabled={!session.releasable || releasing}
        title={t('emulationPage.actions.release')}
        aria-label={t('emulationPage.actions.releaseSession', { source: session.sourceName })}
      >
        {releasing ? <LoaderCircle className="animate-spin" /> : <Power />}
      </Button>
    </div>
  );
}

function MetricsBand({ model }: EmulationWorkspaceProps) {
  const { t } = useTranslation();
  const metrics = [
    ['sources', model.metrics.sourceCount],
    ['active', model.metrics.activeCount],
    ['running', model.metrics.runningCount],
    ['failed', model.metrics.failedCount],
  ] as const;
  return (
    <div className="grid grid-cols-2 border-b border-forensics-border bg-forensics-panel md:grid-cols-4">
      {metrics.map(([key, value]) => (
        <div key={key} className="min-w-0 border-r border-forensics-border px-5 py-3 last:border-r-0 even:border-r-0 md:even:border-r md:last:border-r-0">
          <div className="font-mono text-lg text-forensics-text">{value}</div>
          <div className="mt-0.5 truncate text-[10px] uppercase text-forensics-muted">
            {t(`emulationPage.metrics.${key}`)}
          </div>
        </div>
      ))}
    </div>
  );
}

function LaunchPanel({ model }: EmulationWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <section className="border-b border-forensics-border p-5 xl:border-b-0 xl:border-r">
      <div className="mb-5 flex items-center gap-2">
        <Disc3 className="size-4 text-forensics-primary-blue" />
        <h2 className="text-[14px] text-forensics-text">{t('emulationPage.launch.title')}</h2>
      </div>

      <div className="space-y-5">
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
        </Field>

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

        {model.preflight && model.preflight.installs.some((install) => install.samPresent || install.platform === 'linux') ? (
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
                {model.preflight.installs
                  .filter((install) => install.samPresent || install.platform === 'linux')
                  .map((install) => (
                    <SelectItem key={install.partitionIndex} value={String(install.partitionIndex)}>
                      [P{install.partitionIndex}]
                      {install.platform === 'linux' ? ` · ${t('emulationPage.preflight.linuxInstall')}` : ''}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
            {model.bypassPartition !== undefined && model.bypassIsLinux ? (
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
            {model.bypassPartition !== undefined && !model.bypassIsLinux ? (
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
        ) : null}

        {model.selectedSource ? (
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
        ) : null}

        {model.preflight && model.preflight.installs.length > 0 ? (
          <div className="space-y-1.5 border-y border-forensics-border py-3">
            <div className="text-[11px] text-forensics-muted">{t('emulationPage.preflight.title')}</div>
            {model.preflight.installs.map((install) => (
              <div key={install.partitionIndex} className="flex items-center gap-2 text-[11px]">
                <span className="font-mono text-forensics-text">[P{install.partitionIndex}]</span>
                {install.platform === 'linux' ? (
                  <>
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
                  </>
                ) : (
                  <>
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
                  </>
                )}
              </div>
            ))}
            {model.preflight.installs.some((install) => install.platform === 'linux') ? (
              <div className="pt-1 text-[10px] leading-4 text-forensics-muted">
                {t('emulationPage.preflight.linuxBypassHint')}
              </div>
            ) : null}
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
        ) : null}

        {model.preflight ? (
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
        ) : null}

        {model.recoveryIsoPath && model.preflight?.maintenanceToolAvailable === false ? (
          <div className="border-b border-forensics-border py-3 text-[11px] text-forensics-warning-text">
            {t('emulationPage.maintenance.peWithoutTool')}
          </div>
        ) : null}

        <div className="border border-forensics-sakura-300 bg-forensics-sakura-100/20 p-3">
          <div className="flex items-center gap-2 text-[11px] text-forensics-text">
            <ShieldCheck className="size-4 text-forensics-primary-blue" />
            {t('emulationPage.protection.title')}
          </div>
          <div className="mt-1 text-[11px] leading-5 text-forensics-muted">
            {t('emulationPage.protection.detail')}
          </div>
        </div>

        {model.error ? (
          <div className="flex items-start gap-2 border border-forensics-error-border bg-forensics-error-bg p-3 text-[11px] text-forensics-error-text">
            <CircleAlert className="mt-0.5 size-4 shrink-0" />
            <span className="min-w-0 break-words">{model.error}</span>
          </div>
        ) : null}

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
      </div>
    </section>
  );
}

function SessionsPanel({ model }: EmulationWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <section className="min-w-0 p-5">
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h2 className="text-[14px] text-forensics-text">{t('emulationPage.sessions.title')}</h2>
          <div className="mt-1 text-[10px] text-forensics-muted">{t('emulationPage.sessions.count', { count: model.sessions.length })}</div>
        </div>
      </div>
      {model.sessions.length === 0 ? (
        <div className="flex min-h-48 items-center justify-center border-y border-forensics-border text-[11px] text-forensics-muted">
          {t('emulationPage.sessions.empty')}
        </div>
      ) : (
        <div className="overflow-x-auto border border-forensics-border">
          <div className="grid min-w-[760px] grid-cols-[minmax(180px,1.4fr)_140px_120px_minmax(180px,1fr)_52px] border-b border-forensics-border bg-forensics-panel px-4 py-2 text-[10px] uppercase text-forensics-muted">
            <span>{t('emulationPage.sessions.columns.source')}</span>
            <span>{t('emulationPage.sessions.columns.state')}</span>
            <span>{t('emulationPage.sessions.columns.size')}</span>
            <span>{t('emulationPage.sessions.columns.control')}</span>
            <span className="sr-only">{t('emulationPage.sessions.columns.action')}</span>
          </div>
          {model.sessions.map((session) => (
            <SessionRow
              key={session.sessionId}
              session={session}
              releasing={model.releasingSessionId === session.sessionId}
              onRelease={model.release}
            />
          ))}
        </div>
      )}
    </section>
  );
}

export function EmulationWorkspace({ model }: EmulationWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <header className="flex shrink-0 items-center justify-between gap-4 border-b border-forensics-border bg-forensics-panel px-6 py-4">
        <div className="min-w-0">
          <h1 className="truncate text-xl text-forensics-text">{t('emulationPage.title')}</h1>
          <div className="mt-1 truncate font-mono text-[10px] text-forensics-muted">
            {model.caseName ?? t('emulationPage.noCase')}
          </div>
        </div>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={() => void model.refresh()}
          disabled={!model.hasCase || model.refreshing}
          title={t('emulationPage.actions.refresh')}
          aria-label={t('emulationPage.actions.refresh')}
        >
          <RefreshCw className={model.refreshing ? 'animate-spin' : undefined} />
        </Button>
      </header>

      {!model.hasCase && model.caseLoaded ? (
        <div className="flex flex-1 items-center justify-center p-6 text-[12px] text-forensics-muted">
          {t('emulationPage.emptyCase')}
        </div>
      ) : model.loading ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-[12px] text-forensics-muted">
          <LoaderCircle className="size-4 animate-spin" />
          {t('emulationPage.loading')}
        </div>
      ) : (
        <ScrollArea className="min-h-0 flex-1" viewportClassName="min-h-full">
          <MetricsBand model={model} />
          <div className="grid min-h-0 grid-cols-1 xl:grid-cols-[380px_minmax(0,1fr)]">
            <LaunchPanel model={model} />
            <SessionsPanel model={model} />
          </div>
        </ScrollArea>
      )}
    </div>
  );
}
