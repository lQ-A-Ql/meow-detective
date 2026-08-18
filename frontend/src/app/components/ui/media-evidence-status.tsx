import { FileWarning, KeyRound, Link2Off } from 'lucide-react';
import { Badge } from './badge';
import { cn } from './utils';

export type MediaEvidenceStatusKind = 'linked-encrypted' | 'linked-unavailable' | 'unlinked';

export interface MediaEvidenceStatusProps {
  status: MediaEvidenceStatusKind;
  label: string;
  detail?: string;
  className?: string;
}

const statusIcons = {
  'linked-encrypted': KeyRound,
  'linked-unavailable': FileWarning,
  unlinked: Link2Off,
} satisfies Record<MediaEvidenceStatusKind, typeof KeyRound>;

export function MediaEvidenceStatus({
  status,
  label,
  detail,
  className,
}: MediaEvidenceStatusProps) {
  const Icon = statusIcons[status];
  return (
    <div className={cn('min-w-0 text-left', className)} data-media-evidence-status={status}>
      <Badge variant="outline" className="max-w-full gap-1 text-[9px] leading-4 whitespace-normal">
        <Icon />
        <span>{label}</span>
      </Badge>
      {detail ? <div className="mt-1 break-all font-mono text-[9px] text-forensics-muted">{detail}</div> : null}
    </div>
  );
}
