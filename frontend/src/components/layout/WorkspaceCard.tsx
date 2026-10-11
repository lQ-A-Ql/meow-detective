import type { HTMLAttributes, ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

/** Shared outer card primitive for workspace regions. Keeps radius, spacing and elevation aligned. */
export function WorkspaceCard({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLElement> & { children: ReactNode }) {
  return (
    <section
      className={cn(
        'min-w-0 overflow-hidden rounded-none rounded-bl-shell border border-forensics-line/70 bg-forensics-paper shadow-[0_2px_8px_rgba(32,27,28,0.04)]',
        className,
      )}
      {...props}
    >
      {children}
    </section>
  );
}
