import { Clock3 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import type { TraceItem } from '@/types/models';

export function BottomDrawerTrace({ trace }: { trace: TraceItem[] }) {
  const { t } = useTranslation();
  return (
    <ScrollArea className="min-h-0" viewportClassName="p-3">
      <div className="mb-2 flex items-center justify-between text-[10px] font-light uppercase tracking-wider text-forensics-text-tertiary">
        <span>{t('bottomDrawer.trace.title')}</span>
        <span className="font-mono text-forensics-muted-light">{t('bottomDrawer.trace.recentStream')}</span>
      </div>
      <div className="space-y-2 text-[11px]">
        {trace.map((item) => (
          <div key={item.id} className="flex gap-2 border-b border-forensics-border-light pb-2 text-forensics-text-tertiary">
            <Clock3 size={11} className="mt-0.5 shrink-0 text-forensics-muted-lighter" />
            <div>
              <div className="font-mono text-forensics-muted-light">{item.ts}</div>
              <div>{item.message}</div>
            </div>
          </div>
        ))}
      </div>
    </ScrollArea>
  );
}
