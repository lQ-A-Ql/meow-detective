import {
  LoaderCircle,
  Power,
  RefreshCw,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { LinuxLaunchPanel } from '@/features/emulation/components/LinuxLaunchPanel';
import { WindowsLaunchPanel } from '@/features/emulation/components/WindowsLaunchPanel';
import type {
  EmulationSessionView,
  EmulationWorkspaceModel,
} from '@/features/emulation/use-emulation-workspace-model';
import { formatBytes } from '@/lib/format-bytes';
import type { EmulationState } from '@/types/models';

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
          {t(`emulationPage.guestPhases.${session.guestPhase ?? 'unknown'}`)}
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
  return model.selectedSource?.platform === 'LINUX'
    ? <LinuxLaunchPanel model={model} />
    : <WindowsLaunchPanel model={model} />;
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
