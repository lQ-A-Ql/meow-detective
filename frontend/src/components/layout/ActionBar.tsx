import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

/** Pinned form footer: the apply/reset pair plus the current save state. */
export function ActionBar({ children, note, className }: { children: ReactNode; note?: ReactNode; className?: string }) {
  return (
    <div className={cn('workspace-content flex shrink-0 items-center justify-end gap-3 bg-transparent', className)}>
      {note ? <div className="mr-auto text-[11px] text-forensics-muted" aria-live="polite">{note}</div> : null}
      {children}
    </div>
  );
}
