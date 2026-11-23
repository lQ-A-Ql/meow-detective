import type { HTMLAttributes } from 'react';
import { cn } from './utils';

export type StatusNoticeTone = 'neutral' | 'warning' | 'error';

const toneClasses: Record<StatusNoticeTone, string> = {
  neutral: 'border-forensics-border bg-forensics-panel text-forensics-muted',
  warning: 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text',
  error: 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text',
};

export function StatusNotice({
  tone = 'neutral',
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & { tone?: StatusNoticeTone }) {
  return (
    <div role="status" className={cn('border-b px-4 py-3 text-[11px]', toneClasses[tone], className)} {...props} />
  );
}
