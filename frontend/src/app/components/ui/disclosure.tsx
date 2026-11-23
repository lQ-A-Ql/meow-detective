import type { ComponentProps, ReactNode } from 'react';
import { cn } from './utils';

export function Disclosure({
  summary,
  children,
  className,
  ...props
}: ComponentProps<'details'> & { summary: ReactNode }) {
  return (
    <details className={cn('mt-2 text-forensics-muted', className)} {...props}>
      <summary className="cursor-pointer focus-visible:outline focus-visible:outline-forensics-text">{summary}</summary>
      <div className="mt-1 break-words whitespace-pre-wrap">{children}</div>
    </details>
  );
}
