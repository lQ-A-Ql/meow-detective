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
      data-slot="workspace-card"
      className={cn(
        'min-w-0 overflow-hidden rounded-none rounded-bl-shell border border-forensics-line/70 bg-forensics-paper shadow-workspace',
        className,
      )}
      {...props}
    >
      {children}
    </section>
  );
}
