import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { ListRow } from '@/components/lists/ListRow';
import type { RecentCase } from '@/types/models';

export function RecentCasesDialog({ open, onOpenChange, recentCases, onOpen, onDelete, intent = 'open' }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  recentCases: RecentCase[];
  onOpen: (item: RecentCase) => void;
  onDelete: (item: RecentCase) => void;
  intent?: 'open' | 'delete';
}) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined}>
        <DialogHeader><DialogTitle>{t(intent === 'delete' ? 'caseHome.confirm.chooseCaseToDelete' : 'caseHome.welcome.recent.title')}</DialogTitle></DialogHeader>
        <ScrollArea className="min-w-0" viewportClassName="max-h-[50vh]">
          <div className="space-y-1">
            {recentCases.length === 0 ? <p className="py-4 text-[12px] text-forensics-muted">{t('caseHome.welcome.recent.empty')}</p> : null}
            {recentCases.map((item) => (
              <div key={`${item.caseRoot}-${item.openedAt}`} className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-2">
                <ListRow ariaLabel={item.name} wrapLabel onActivate={() => intent === 'delete' ? onDelete(item) : onOpen(item)} className="h-auto min-h-14 py-2" label={<><span className="block truncate text-[12px] text-forensics-ink">{item.name}</span><span className="mt-1 block truncate font-mono text-[10px] text-forensics-muted">{item.caseRoot}</span></>} />
                <IconButton variant="destructive" label={t('caseHome.actions.deleteCase')} onClick={() => onDelete(item)}><ActionIcon name="trash" /></IconButton>
              </div>
            ))}
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
