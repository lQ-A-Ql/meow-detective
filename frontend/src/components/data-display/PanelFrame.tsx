import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

export function PanelFrame({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <section
      className={cn(
        'min-w-0 border-b border-forensics-divider bg-transparent p-4 last:border-b-0',
        className,
      )}
    >
      {children}
    </section>
  );
}

/**
 * Empty and no-data surface. A state that offers the investigator nothing to do is a defect:
 * pass `action` whenever the page has a next step (import, run analysis, clear a filter).
 */
export function EmptyState({
  title,
  description,
  action,
  children,
  className,
}: {
  title?: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn('bg-transparent p-6 text-center text-[12px] text-forensics-ink-3', className)}>
      {title ? <div className="text-[13px] text-forensics-ink">{title}</div> : null}
      {description ? <div className="mt-1 leading-5">{description}</div> : null}
      {children}
      {action ? <div className="mt-3 flex justify-center gap-2">{action}</div> : null}
    </div>
  );
}
