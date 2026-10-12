import { ConfirmationDialog } from '@/app/components/ui/confirmation-dialog';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { ListRow } from '@/components/lists/ListRow';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { TFunction } from 'i18next';

export interface McpServerItemData {
  id: string;
  name: string;
  transportType: 'sse' | 'stdio';
  url?: string;
  command?: string;
  enabled: boolean;
  connected: boolean;
  hasResources: boolean;
  hasTools: boolean;
  hasPrompts: boolean;
  lastError?: string;
}
interface Props {
  server: McpServerItemData;
  isSelected: boolean;
  loading: boolean;
  confirmRemove: boolean;
  t: TFunction;
  onConnect: () => void;
  onRemove: () => void;
  onSelect: () => void;
  onConfirmChange: (open: boolean) => void;
}

export function McpServerItemView({ server, isSelected, loading, confirmRemove, t, onConnect, onRemove, onSelect, onConfirmChange }: Props) {
  const status = server.connected ? 'mcpUi.server.connected' : server.lastError ? 'mcpUi.server.error' : 'mcpUi.server.disconnected';
  return (
    <>
      <div className="flex min-w-0 items-center gap-2 py-1">
        <ListRow selected={isSelected} wrapLabel ariaCurrent={isSelected} onActivate={onSelect} ariaLabel={server.name}
          className="min-w-0 flex-1" icon={<ActionIcon name="plug" />}
          label={<>
            <span className="block truncate text-[12px] text-forensics-ink">{server.name}</span>
            <span className="block truncate font-mono text-[11px] text-forensics-muted" title={server.transportType === 'sse' ? server.url : server.command}>
              {server.transportType === 'sse' ? server.url : server.command}
            </span>
          </>}
          meta={<span className="hidden shrink-0 text-[10px] text-forensics-muted sm:inline">
            {[
              server.hasResources ? t('mcpUi.server.resources') : null,
              server.hasTools ? t('mcpUi.server.tools') : null,
              server.hasPrompts ? t('mcpUi.server.prompts') : null,
            ].filter(Boolean).join(' · ')}
          </span>} />
        <StatusBadge label={t(status)} tone={server.connected ? 'success' : server.lastError ? 'error' : 'neutral'} />
        <IconButton label={t(server.connected ? 'mcpUi.server.disconnect' : 'mcpUi.server.connect')}
          onClick={onConnect} disabled={loading} pressed={server.connected}>
          <ActionIcon name={server.connected ? 'pause' : 'play'} />
        </IconButton>
        <IconButton label={t('mcpUi.server.delete')} variant="forensicsDangerGhost" onClick={() => onConfirmChange(true)}>
          <ActionIcon name="trash" />
        </IconButton>
      </div>
      <ConfirmationDialog open={confirmRemove} onOpenChange={onConfirmChange}
        title={t('mcpUi.server.deleteTitle')} description={t('mcpUi.server.deleteDescription', { name: server.name })}
        cancelLabel={t('common.cancel')} confirmLabel={t('mcpUi.server.delete')} destructive onConfirm={onRemove} />
    </>
  );
}
