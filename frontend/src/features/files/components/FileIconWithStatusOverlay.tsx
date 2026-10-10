import { useTranslation } from 'react-i18next';
import { EyeOff, X } from 'lucide-react';
import { getFileIcon } from '@/lib/file-icons';
import { TokenIcon } from '@/components/data-display';

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
  const { t } = useTranslation();
  const iconInfo = getFileIcon({ name, entryType, status, expanded });
  const IconComponent = iconInfo.icon;
  const statusParts: string[] = [];
  if (deleted) statusParts.push(t('wechatWorkspace.deleted'));
  if (hidden) statusParts.push(t('fileBrowser.status.hidden'));
  if (system) statusParts.push(t('fileBrowser.status.system'));
  const title = statusParts.length ? statusParts.join(' / ') : undefined;

  return (
    <span
      className={`relative inline-flex shrink-0 items-center justify-center ${className}`}
      style={{ width: size + 6, height: size + 6 }}
      title={title}
      aria-label={title}
      data-deleted={deleted ? 'true' : undefined}
      data-hidden={hidden || system ? 'true' : undefined}
    >
      <TokenIcon icon={IconComponent} size={size} color={iconInfo.color} />
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
