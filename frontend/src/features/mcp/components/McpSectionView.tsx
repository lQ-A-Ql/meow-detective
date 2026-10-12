import { Button } from '@/app/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import type { TFunction } from 'i18next';
import { createElement, type ReactNode, type ComponentType } from 'react';
import type { McpServerItemData } from './McpServerItemView';

type McpServerRowProps = {
  server: McpServerItemData;
  isSelected: boolean;
  onConnect: () => void;
  onDisconnect: () => void;
  onRemove: () => void;
  onSelect: () => void;
};
type Row = McpServerItemData & { component: ComponentType<McpServerRowProps> };
interface Props {
  t: TFunction;
  expanded: boolean;
  loading: boolean;
  onToggle: () => void;
  onAdd: () => void;
  servers: Row[];
  selectedServerId: string | null;
  onSelect: (id: string) => void;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onRemove: (id: string) => void;
  hostToolList?: ReactNode;
  resourceList?: ReactNode;
  toolList?: ReactNode;
  error: string | null;
  dialog?: ReactNode;
}

export function McpSectionView({ t, expanded, loading, onToggle, onAdd, servers, selectedServerId, onSelect, onConnect,
  onDisconnect, onRemove, hostToolList, resourceList, toolList, error, dialog }: Props) {
  return (
    <section className="max-w-4xl space-y-5">
      <Button type="button" variant="forensicsGhost" shape="row" aria-expanded={expanded} onClick={onToggle}
        className="h-7 gap-2 px-0 text-[14px] text-forensics-ink">
        <ActionIcon name={expanded ? 'chevron-down' : 'chevron-right'} className="size-3" />
        {t('settings.sections.mcp.title')}
        {loading ? <span className="text-[11px] text-forensics-muted">{t('settings.sections.mcp.loading')}</span> : null}
      </Button>
      {expanded ? (
        <div className="space-y-6">
          {hostToolList}
          <div className="space-y-2">
            <div className="flex items-center gap-3">
              <h3 className="text-[12px] text-forensics-ink">{t('settings.sections.mcp.connectionTitle')}</h3>
              <span className="text-[11px] text-forensics-muted">
                {t('settings.sections.mcp.connectionStatus', { count: servers.filter((server) => server.connected).length })}
              </span>
              <IconButton label={t('settings.sections.mcp.addServer')} onClick={onAdd} className="ml-auto">
                <ActionIcon name="plus" />
              </IconButton>
            </div>
            {servers.length === 0 ? (
              <div className="py-3 text-[12px] text-forensics-muted">{t('settings.sections.mcp.noServers')}</div>
            ) : servers.map((server) => <div key={server.id}>{createElement(server.component, {
              server, isSelected: server.id === selectedServerId, onConnect: () => onConnect(server.id),
              onDisconnect: () => onDisconnect(server.id), onRemove: () => onRemove(server.id), onSelect: () => onSelect(server.id),
            })}</div>)}
          </div>
          {selectedServerId ? (
            <Tabs key={selectedServerId} defaultValue="resources" className="min-w-0 gap-3">
              <TabsList aria-label={t('settings.sections.mcp.connectionTitle')}>
                <TabsTrigger value="resources">{t('mcpUi.resources.title')}</TabsTrigger>
                <TabsTrigger value="tools">{t('mcpUi.tools.title')}</TabsTrigger>
              </TabsList>
              <TabsContent value="resources">{resourceList}</TabsContent>
              <TabsContent value="tools">{toolList}</TabsContent>
            </Tabs>
          ) : null}
          {error ? <div role="alert" className="text-[12px] text-forensics-error-text">{error}</div> : null}
        </div>
      ) : null}
      {dialog}
    </section>
  );
}
