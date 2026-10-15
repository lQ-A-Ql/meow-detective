import {
  LoaderCircle,
  Power,
} from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { SidebarIcon } from '@/components/navigation/SidebarIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { EmulationSettingsDialog } from './EmulationSettingsDialog';
import type {
  EmulationSessionView,
  EmulationWorkspaceModel,
} from '@/features/emulation/use-emulation-workspace-model';
import { formatBytes } from '@/lib/format-bytes';
import type { EmulationState } from '@/types/models';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { BrandWatermark } from '@/components/brand';

interface EmulationWorkspaceProps {
  model: EmulationWorkspaceModel;
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
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

export function EmulationWorkspace({ model, settingsOpen, onSettingsOpenChange }: EmulationWorkspaceProps) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-base">
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
        <ScrollArea className="min-h-0 flex-1" viewportClassName="workspace-scroll-viewport h-full min-h-0 overflow-y-auto scrollbar-none workspace-frame">
          <WorkspaceCard className="workspace-content relative isolate flex h-full min-h-full flex-1 flex-col">
            <div className="mb-5 flex items-start justify-between gap-4">
              <div>
                <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-forensics-ink-3">{t('emulationPage.statusTitle')}</div>
                <h1 className="mt-1 text-xl font-semibold text-forensics-ink">{model.caseName ?? t('emulationPage.title')}</h1>
                <div className="mt-1 text-[11px] text-forensics-muted">{t('emulationPage.activeSummary', { count: model.metrics.activeCount })}</div>
              </div>
              <div className="flex items-center gap-1">
                <Tooltip content={t('emulationPage.actions.settings')}>
                  <Button type="button" variant="forensicsGhost" size="iconSm" aria-label={t('emulationPage.actions.settings')} onClick={() => onSettingsOpenChange(true)} disabled={!model.hasCase}>
                    <SidebarIcon name="settings" className="size-4" />
                  </Button>
                </Tooltip>
                <Tooltip content={t('emulationPage.actions.refresh')}>
                  <Button type="button" variant="forensicsGhost" size="iconSm" aria-label={t('emulationPage.actions.refresh')} onClick={() => void model.refresh()} disabled={!model.hasCase || model.refreshing}>
                    <ActionIcon name="refresh" className={model.refreshing ? 'size-4 animate-spin' : 'size-4'} />
                  </Button>
                </Tooltip>
              </div>
            </div>
            {model.sessions.filter((session) => session.active).length === 0 ? (
              <div className="flex min-h-24 items-center justify-center rounded-ctl bg-forensics-panel-strong px-4 py-8 text-[12px] text-forensics-muted">
                {t('emulationPage.sessions.empty')}
              </div>
            ) : (
              <div className="space-y-2">
                {model.sessions.filter((session) => session.active).map((session) => (
                  <div key={session.sessionId} className="border-b border-forensics-line last:border-b-0">
                    <SessionRow session={session} releasing={model.releasingSessionId === session.sessionId} onRelease={model.release} />
                  </div>
                ))}
              </div>
            )}
            {model.metrics.activeCount === 0 ? (
              <>
                <div className="emulation-artwork-space">
                  <BrandWatermark motif="moon-cat" className="ornament-emulation" />
                </div>
                <BrandWatermark motif="emulation" className="watermark-emulation" />
              </>
            ) : null}
          </WorkspaceCard>
        </ScrollArea>
      )}
      <EmulationSettingsDialog model={model} open={settingsOpen} onOpenChange={onSettingsOpenChange} />
    </div>
  );
}
