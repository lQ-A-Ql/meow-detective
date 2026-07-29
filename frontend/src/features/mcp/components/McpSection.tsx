import { useState, type ComponentProps, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Bot, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { McpServerDialog } from './McpServerDialog';
import { McpServerItem } from './McpServerItem';

interface McpSectionProps {
  servers: Array<ComponentProps<typeof McpServerItem>['server']>;
  selectedServerId: string | null;
  loading: boolean;
  error: string | null;
  onAdd: ComponentProps<typeof McpServerDialog>['onAdd'];
  onConnect: (serverId: string) => void;
  onDisconnect: (serverId: string) => void;
  onRemove: (serverId: string) => void;
  onSelect: (serverId: string) => void;
  testConnection: ComponentProps<typeof McpServerDialog>['testConnection'];
  resourceList?: ReactNode;
  toolList?: ReactNode;
}

export function McpSection({
  servers,
  selectedServerId,
  loading,
  error,
  onAdd,
  onConnect,
  onDisconnect,
  onRemove,
  onSelect,
  testConnection,
  resourceList,
  toolList,
}: McpSectionProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const selectedServer = servers.find((server) => server.id === selectedServerId);

  return (
    <section>
      <div
        className="flex items-center gap-2 mb-3 cursor-pointer select-none"
        onClick={() => setExpanded((value) => !value)}
      >
        <Bot size={14} className="text-forensics-muted-light" />
        <span className="text-[13px] font-light text-forensics-text-secondary">{t('settings.sections.mcp.title')}</span>
        {expanded ? <ChevronDown size={14} className="text-forensics-muted-light" /> : <ChevronRight size={14} className="text-forensics-muted-light" />}
        {loading && <span className="text-[10px] text-forensics-info-text">{t('settings.sections.mcp.loading')}</span>}
      </div>

      {expanded && (
        <div className="space-y-4">
          <div className="bg-forensics-input-bg border border-forensics-border p-3">
            <div className="text-[11px] font-light text-forensics-muted mb-2">
              {t('settings.sections.mcp.connectionTitle')}
            </div>
            <div className="space-y-1">
              {servers.length === 0 ? (
                <div className="text-[11px] text-forensics-muted py-2">{t('settings.sections.mcp.noServers')}</div>
              ) : (
                servers.map((server) => (
                  <McpServerItem
                    key={server.id}
                    server={server}
                    isSelected={server.id === selectedServerId}
                    onConnect={() => onConnect(server.id)}
                    onDisconnect={() => onDisconnect(server.id)}
                    onRemove={() => onRemove(server.id)}
                    onSelect={() => onSelect(server.id)}
                  />
                ))
              )}
            </div>
            <Button
              type="button"
              variant="forensicsGhost"
              size="xs"
              onClick={() => setShowAddDialog(true)}
              className="mt-2 text-forensics-info-text hover:text-forensics-info-text"
            >
              <Plus size={12} />
              {t('settings.sections.mcp.addServer')}
            </Button>
          </div>

          {selectedServer && (
            <div className="grid grid-cols-2 gap-4">
              {resourceList}
              {toolList}
            </div>
          )}

          <div className="text-[11px] text-forensics-muted">
            {t('settings.sections.mcp.connectionStatus', { count: servers.filter((server) => server.connected).length })}
          </div>

          {error && (
            <div className="p-3 rounded-none text-[12px] bg-forensics-error-bg text-forensics-error-text border border-forensics-error-border">
              {error}
            </div>
          )}
        </div>
      )}

      {showAddDialog && (
        <McpServerDialog
          onClose={() => setShowAddDialog(false)}
          onAdd={onAdd}
          testConnection={testConnection}
        />
      )}
    </section>
  );
}
