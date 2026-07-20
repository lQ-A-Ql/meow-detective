import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

export interface KeyValueFieldProps {
  label: ReactNode;
  value?: ReactNode;
  mono?: boolean;
  layout?: 'stacked' | 'inline';
  className?: string;
  valueClassName?: string;
  fallback?: ReactNode;
}

export function KeyValueField({
  label,
  value,
  mono = false,
  layout = 'stacked',
  className,
  valueClassName,
  fallback = '-',
}: KeyValueFieldProps) {
  const displayValue = value === undefined || value === null || value === '' ? fallback : value;

  if (layout === 'inline') {
    return (
      <div className={cn('flex min-w-0 gap-2', className)}>
        <span className="shrink-0 text-forensics-muted-light">{label}:</span>
        <span
          className={cn('min-w-0 truncate text-forensics-text-secondary', mono && 'font-mono text-[10px]', valueClassName)}
          title={typeof displayValue === 'string' ? displayValue : undefined}
        >
          {displayValue}
        </span>
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col gap-0.5 border border-forensics-border bg-transparent p-2', className)}>
      <span className="text-forensics-muted-light">{label}</span>
      <span className={cn('text-forensics-text-secondary', mono && 'font-mono text-[10px]', valueClassName)}>
        {displayValue}
      </span>
    </div>
  );
}
