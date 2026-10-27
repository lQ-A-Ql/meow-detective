import type { TFunction } from 'i18next';
import { Input } from '@/app/components/ui/input';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import type { McpHostClient, McpHostCreatedClient } from '@/types/mcpHost';

interface Props {
  t: TFunction;
  clients: Array<McpHostClient & { expiry: string }>;
  name: string;
  setName: (value: string) => void;
  hours: string;
  setHours: (value: string) => void;
  readEvidence: boolean;
  dataSourceIds: string;
  setDataSourceIds: (value: string) => void;
  created?: McpHostCreatedClient;
  closeCreated: () => void;
  busy: boolean;
  canCreate: boolean;
  create: () => void;
  revoke: (id: string) => void;
  error?: string;
  onCopy: () => void;
  copied: boolean;
  copyError: boolean;
}

export function McpHostAccessView(p: Props) {
  const { t } = p;
  return <div className="space-y-3 text-[12px]">
    <h3 className="text-forensics-ink">{t('mcpUi.access.title')}</h3>
    <div className="flex flex-wrap items-end gap-3">
      <label className="min-w-40 flex-1 text-forensics-muted">{t('mcpUi.access.name')}
        <Input value={p.name} maxLength={100} onChange={(event) => p.setName(event.target.value)} disabled={p.busy} />
      </label>
      <label className="w-28 text-forensics-muted">{t('mcpUi.access.hours')}
        <Input type="number" min={1} max={720} value={p.hours} onChange={(event) => p.setHours(event.target.value)} disabled={p.busy} />
      </label>
      {p.readEvidence ? <label className="min-w-64 flex-1 text-forensics-muted">{t('mcpUi.access.dataSources')}
        <Input value={p.dataSourceIds} placeholder={t('mcpUi.access.dataSourcesPlaceholder')} onChange={(event) => p.setDataSourceIds(event.target.value)} disabled={p.busy} />
      </label> : null}
      <IconButton label={t('mcpUi.access.create')} disabled={p.busy || !p.canCreate} onClick={p.create}><ActionIcon name="plus" /></IconButton>
    </div>
    {p.error ? <div role="alert" className="text-forensics-error-text">{p.error}</div> : null}
    {p.clients.map((client) => <div key={client.id} className="flex items-center gap-3 text-forensics-muted">
      <span className="min-w-0 flex-1 truncate text-forensics-ink">{client.name}</span>
      <span>{t(client.readEvidence ? 'mcpUi.access.contentScope' : 'mcpUi.access.queryScope')}</span>
      {client.readEvidence ? <span>{t('mcpUi.access.dataSourceCount', { count: client.allowedDataSourceIds.length })}</span> : null}
      {client.exportGrants.length > 0 ? <span>{t('mcpUi.access.exportCount', { count: client.exportGrants.length })}</span> : null}
      <span>{t('mcpUi.access.expires', { value: client.expiry })}</span>
      <IconButton label={t('mcpUi.access.revoke', { name: client.name })} onClick={() => p.revoke(client.id)} disabled={p.busy}><ActionIcon name="close" /></IconButton>
    </div>)}
    <Dialog open={Boolean(p.created)} onOpenChange={(open) => { if (!open) p.closeCreated(); }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('mcpUi.access.credential')}</DialogTitle>
          <DialogDescription>{t('mcpUi.access.credentialDescription')}</DialogDescription>
        </DialogHeader>
        <div className="flex items-center gap-2">
          <Input aria-label={t('mcpUi.access.credential')} type="password" readOnly value={p.created?.token ?? ''} />
          <IconButton label={t(p.copied ? 'mcpUi.host.copied' : 'mcpUi.access.copy')} onClick={p.onCopy}><ActionIcon name={p.copied ? 'check' : 'copy'} /></IconButton>
        </div>
        {p.copyError ? <div role="alert" className="text-forensics-error-text">{t('mcpUi.host.copyFailed')}</div> : null}
      </DialogContent>
    </Dialog>
  </div>;
}
