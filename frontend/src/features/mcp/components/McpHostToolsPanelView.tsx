import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { StatusBadge } from '@/components/status/StatusBadge';
import { Checkbox } from '@/app/components/ui/checkbox';
import type { McpHostStatus } from '@/types/models';
import type { TFunction } from 'i18next';
import type { ReactNode } from 'react';

interface Props {
  t: TFunction;
  status?: McpHostStatus;
  loading: boolean;
  copied: boolean;
  copyError: boolean;
  onEnable: (enabled: boolean) => void;
  onCopy: () => void;
  onRetry: () => void;
  toolList: ReactNode;
  access?: ReactNode;
  activity?: ReactNode;
}

export function McpHostToolsPanelView({ t, status, loading, copied, copyError, onEnable, onCopy, onRetry, toolList, access, activity }: Props) {
  const settings = status?.settings;
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-2 text-[12px] text-forensics-ink">
          <Checkbox aria-label={t('mcpUi.host.enable')} checked={settings?.enabled ?? false} disabled={loading || !settings}
            onCheckedChange={(checked) => onEnable(checked === true)} />
          {t('mcpUi.host.enable')}
        </label>
        <span role="status">
          <StatusBadge label={t(status?.running ? 'mcpUi.host.running' : settings?.enabled ? 'mcpUi.host.starting' : 'mcpUi.host.stopped')}
            tone={status?.running ? 'success' : 'neutral'} />
        </span>
        <div className="flex min-w-0 flex-1 items-center gap-1">
          <code className="min-w-0 break-all text-[11px] text-forensics-muted">{status?.endpoint}</code>
          <IconButton label={t(copied ? 'mcpUi.host.copied' : 'mcpUi.host.copyEndpoint')} disabled={!status} onClick={onCopy}>
            <ActionIcon name={copied ? 'check' : 'copy'} />
          </IconButton>
          {settings?.enabled && !status?.running ? (
            <IconButton label={t('mcpUi.host.retry')} disabled={loading} onClick={onRetry}><ActionIcon name="refresh" /></IconButton>
          ) : null}
        </div>
      </div>
      {copyError ? <div role="alert" className="text-[11px] text-forensics-error-text">{t('mcpUi.host.copyFailed')}</div> : null}
      {access}
      {activity}
      {toolList}
    </div>
  );
}
