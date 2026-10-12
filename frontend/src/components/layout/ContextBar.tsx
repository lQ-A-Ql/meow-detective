import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

/**
 * The single page-level control bar, rendered directly under the activity rail.
 *
 * Pages own no title bar: scope, filters and actions share this compact strip so every
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
        'flex h-9 shrink-0 items-center gap-3 overflow-hidden bg-transparent px-4',
        justify,
        className,
      )}
    >
      {children}
    </div>
  );
}
