import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

export function PanelFrame({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <section className={cn('rounded-none border border-forensics-border bg-transparent p-5 transition-colors', className)}>
      {children}
    </section>
  );
}

export function EmptyState({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('rounded-none border border-dashed border-forensics-border-strong bg-transparent p-6 text-center text-[12px] text-forensics-muted-light', className)}>
      {children}
    </div>
  );
}
