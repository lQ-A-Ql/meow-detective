import { ActionIcon } from '@/components/controls/ActionIcon';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import type { LinuxBypassOutcomePresentation } from '../linux-bypass-outcome-model';

interface LinuxBypassOutcomePanelViewProps {
  presentation?: LinuxBypassOutcomePresentation;
  busy: boolean;
  onLaunch: () => Promise<void>;
  onRelease: () => Promise<void>;
}

export function LinuxBypassOutcomePanelView({
  presentation,
  busy,
  onLaunch,
  onRelease,
}: LinuxBypassOutcomePanelViewProps) {
  if (!presentation) return null;
  return (
    <WorkspaceCard className="min-w-0 p-3">
      <fieldset className="space-y-3">
        <legend className="flex items-center gap-2 text-[12px] text-forensics-warning-text">
          <ActionIcon name="warning" className="size-4" />
          {presentation.title}
        </legend>
        <p className="text-[11px] leading-5 text-forensics-muted">{presentation.description}</p>
        {presentation.reason ? (
          <div className="flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-forensics-muted">{presentation.reasonLabel}</span>
            <Badge variant="secondary">{presentation.reason}</Badge>
          </div>
        ) : null}
        {presentation.details.length > 0 ? (
          <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-[11px]">
            {presentation.details.map((detail) => (
              <div key={detail.key}>
                <dt className="text-forensics-muted">{detail.label}</dt>
                <dd className="text-forensics-text">{detail.value}</dd>
              </div>
            ))}
          </dl>
        ) : null}
        <div className="flex flex-wrap gap-2">
          <Button type="button" shape="pill" variant="outline" disabled={busy} onClick={onLaunch}>
            <ActionIcon name="play" />
            {presentation.launchLabel}
          </Button>
          <Button type="button" shape="pill" variant="forensicsDangerGhost" disabled={busy} onClick={onRelease}>
            <ActionIcon name="close" />
            {presentation.releaseLabel}
          </Button>
        </div>
      </fieldset>
    </WorkspaceCard>
  );
}
