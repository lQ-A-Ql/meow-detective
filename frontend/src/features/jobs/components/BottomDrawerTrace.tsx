import { Clock3 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import type { TraceItem } from '@/types/models';

export function BottomDrawerTrace({ trace }: { trace: TraceItem[] }) {
  const { t } = useTranslation();
  return (
    <ScrollArea
      className="min-h-0 min-w-0 overflow-hidden"
      viewportClassName="min-w-0 overflow-x-hidden p-3"
    >
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2 text-[10px] font-light uppercase tracking-wider text-forensics-text-tertiary">
        <span className="shrink-0">{t('bottomDrawer.trace.title')}</span>
        <span className="min-w-0 truncate text-right font-mono text-forensics-muted-light">
          {t('bottomDrawer.trace.recentStream')}
        </span>
      </div>
      <div className="min-w-0 space-y-2 overflow-hidden text-[11px]">
        {trace.map((item) => (
          <div key={item.id} className="flex min-w-0 gap-2 border-b border-forensics-border-light pb-2 text-forensics-text-tertiary">
            <Clock3 size={11} className="mt-0.5 shrink-0 text-forensics-muted-lighter" />
            <div className="min-w-0">
              <div className="font-mono text-forensics-muted-light">{item.ts}</div>
              <div className="break-words">{item.message}</div>
            </div>
          </div>
        ))}
      </div>
    </ScrollArea>
  );
}
