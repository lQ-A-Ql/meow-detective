import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import type { McpHostStatus } from '@/types/models';
import type { TFunction } from 'i18next';
import type { ReactNode } from 'react';

interface Props { t: TFunction; status?: McpHostStatus; loading: boolean; copied: boolean; copyError: boolean; onEnable: (enabled: boolean) => void; onCopy: () => void; onRetry: () => void; toolList: ReactNode }
export function McpHostToolsPanelView({ t, status, loading, copied, copyError, onEnable, onCopy, onRetry, toolList }: Props) {
  const settings = status?.settings;
  return <div className="space-y-2"><div className="flex flex-wrap items-center gap-3 border border-forensics-border bg-forensics-panel p-3">
    <label className="flex items-center gap-2 text-[11px] text-forensics-text"><Checkbox aria-label={t('mcpUi.host.enable')} checked={settings?.enabled ?? false} disabled={loading || !settings} onCheckedChange={(checked) => onEnable(checked === true)} variant="forensics" />{t('mcpUi.host.enable')}</label>
    <span className="text-[10px] text-forensics-muted" role="status">{t(status?.running ? 'mcpUi.host.running' : settings?.enabled ? 'mcpUi.host.starting' : 'mcpUi.host.stopped')}</span>
    <code className="min-w-0 break-all text-[10px] text-forensics-muted">{status?.endpoint}</code>
    <Button type="button" variant="forensicsGhost" size="compact" disabled={!status} onClick={onCopy}>{t(copied ? 'mcpUi.host.copied' : 'mcpUi.host.copyEndpoint')}</Button>
    {status && settings?.enabled && !status.running ? <Button type="button" variant="forensicsOutline" size="compact" disabled={loading} onClick={onRetry}>{t('mcpUi.host.retry')}</Button> : null}
    {copyError ? <span role="alert" className="text-[10px] text-forensics-error-text">{t('mcpUi.host.copyFailed')}</span> : null}
  </div>{toolList}</div>;
}
