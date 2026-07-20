import { EyeOff, X } from 'lucide-react';
import { getFileIcon } from '@/lib/file-icons';

export interface FileIconWithStatusOverlayProps {
  name: string;
  entryType?: string;
  status?: string;
  expanded?: boolean;
  deleted?: boolean;
  hidden?: boolean;
  system?: boolean;
  size?: number;
  className?: string;
}

function statusTitle({
  deleted,
  hidden,
  system,
}: Pick<FileIconWithStatusOverlayProps, 'deleted' | 'hidden' | 'system'>) {
  const parts = [];
  if (deleted) parts.push('已删除');
  if (hidden) parts.push('隐藏');
  if (system) parts.push('系统');
  return parts.length ? parts.join(' / ') : undefined;
}

export function FileIconWithStatusOverlay({
  name,
  entryType,
  status,
  expanded,
  deleted = false,
  hidden = false,
  system = false,
  size = 12,
  className = '',
}: FileIconWithStatusOverlayProps) {
  const iconInfo = getFileIcon({ name, entryType, status, expanded });
  const IconComponent = iconInfo.icon;
  const title = statusTitle({ deleted, hidden, system });

  return (
    <span
      className={`relative inline-flex shrink-0 items-center justify-center ${className}`}
      style={{ width: size + 6, height: size + 6 }}
      title={title}
      aria-label={title}
      data-deleted={deleted ? 'true' : undefined}
      data-hidden={hidden || system ? 'true' : undefined}
    >
      <IconComponent size={size} style={{ color: iconInfo.color }} />
      {hidden || system ? (
        <span className="absolute -right-0.5 -top-0.5 flex size-2.5 items-center justify-center rounded-none border border-forensics-surface bg-forensics-text-tertiary text-white">
          <EyeOff size={7} strokeWidth={2.5} aria-hidden="true" />
        </span>
      ) : null}
      {deleted ? (
        <span className="absolute -bottom-0.5 -right-0.5 flex size-2.5 items-center justify-center rounded-none border border-forensics-surface bg-forensics-error-text text-white">
          <X size={7} strokeWidth={3} aria-hidden="true" />
        </span>
      ) : null}
    </span>
  );
}
