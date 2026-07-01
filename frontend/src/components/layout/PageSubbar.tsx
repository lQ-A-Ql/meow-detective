import { PropsWithChildren, ReactNode } from 'react';

interface PageSubbarProps extends PropsWithChildren {
  title?: ReactNode;
  meta?: ReactNode;
}

export function PageSubbar({ title, meta, children }: PageSubbarProps) {
  return (
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel">
      {title || meta ? (
        <div className="flex min-h-8 items-center justify-between gap-4 border-b border-forensics-border-light px-4 py-1.5 text-[10px] uppercase tracking-wider text-forensics-muted">
          <div className="min-w-0 truncate font-semibold text-forensics-text-tertiary">{title}</div>
          <div className="min-w-0 truncate font-mono text-forensics-muted-light">{meta}</div>
        </div>
      ) : null}
      {children}
    </div>
  );
}
