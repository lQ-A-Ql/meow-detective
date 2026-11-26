import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/app/components/ui/dialog';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { ActionIcon } from '@/components/controls/ActionIcon';
import type { LogWorkspaceModel } from '../use-log-workspace-model';

export function LogDetailsView({ model }: { model: LogWorkspaceModel }) {
  const { t } = useTranslation();
  const record = model.selected;
  return (
    <Dialog
      open={Boolean(record)}
      onOpenChange={(open) => {
        if (!open) model.closeDetails();
      }}
    >
      <DialogContent className="log-details">
        <DialogHeader>
          <DialogTitle>{t('logs.details')}</DialogTitle>
          <DialogDescription>{record ? `${record.timestamp} · ${record.level.toUpperCase()}` : ''}</DialogDescription>
        </DialogHeader>
        {record ? (
          <ScrollArea className="min-h-0 max-h-[60vh]">
            <pre className="log-details-message">{record.message}</pre>
            <dl className="log-details-fields">
              <dt>{t('logs.module')}</dt>
              <dd>{record.target}</dd>
              <dt>{t('logs.sequence')}</dt>
              <dd>{record.sequence}</dd>
              <dt>{t('logs.thread')}</dt>
              <dd>{record.threadName ?? record.threadId}</dd>
              {Object.entries(record.fields).map(([key, value]) => (
                <div key={key} className="contents">
                  <dt>{key}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
            {record.truncated ? <p>{t('logs.truncated')}</p> : null}
          </ScrollArea>
        ) : null}
        <Button shape="pill" variant="forensicsSurface" onClick={model.copySelected}>
          <ActionIcon name="copy" />
          {model.copyStatus ?? t('logs.copy')}
        </Button>
      </DialogContent>
    </Dialog>
  );
}
