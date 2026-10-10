import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

/**
 * The single page-level control bar, rendered directly under the activity rail.
 *
 * Pages own no title bar: scope, filters and actions all live in this 44px strip so every
 * workbench page presents the same geometry. Pages may use at most one ContextBar.
 */
export function ContextBar({
  children,
  align = 'between',
  className,
}: {
  children: ReactNode;
  align?: 'start' | 'between' | 'end';
  className?: string;
}) {
  const justify = align === 'start' ? 'justify-start' : align === 'end' ? 'justify-end' : 'justify-between';
  return (
    <div
      className={cn(
        'flex h-11 shrink-0 items-center gap-3 overflow-hidden border-b border-forensics-border bg-forensics-panel-strong px-4',
        justify,
        className,
      )}
    >
      {children}
    </div>
  );
}
