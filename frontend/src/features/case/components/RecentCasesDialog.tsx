import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { ListRow } from '@/components/lists/ListRow';
import type { RecentCase } from '@/types/models';

export function RecentCasesDialog({ open, onOpenChange, recentCases, onOpen, onDelete }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  recentCases: RecentCase[];
  onOpen: (item: RecentCase) => void;
  onDelete: (item: RecentCase) => void;
}) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined}>
        <DialogHeader><DialogTitle>{t('caseHome.welcome.recent.title')}</DialogTitle></DialogHeader>
        <ScrollArea className="max-h-[50vh]">
          <div className="space-y-1">
            {recentCases.map((item) => (
              <div key={`${item.caseRoot}-${item.openedAt}`} className="flex items-center gap-2">
                <ListRow ariaLabel={item.name} wrapLabel onActivate={() => onOpen(item)} className="h-auto min-h-14 py-2" label={<><span className="block truncate text-[12px] text-forensics-ink">{item.name}</span><span className="mt-1 block truncate font-mono text-[10px] text-forensics-muted">{item.caseRoot}</span></>} />
                <Tooltip content={t('caseHome.actions.deleteCase')}>
                  <Button type="button" variant="forensicsDangerGhost" title={t('caseHome.actions.deleteCase')} aria-label={t('caseHome.actions.deleteCase')} onClick={() => onDelete(item)}><ActionIcon name="trash" /></Button>
                </Tooltip>
              </div>
            ))}
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
