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
        <span className="shrink-0 text-[#888]">{label}:</span>
        <span
          className={cn('min-w-0 truncate text-[#333]', mono && 'font-mono text-[10px]', valueClassName)}
          title={typeof displayValue === 'string' ? displayValue : undefined}
        >
          {displayValue}
        </span>
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col gap-0.5 border border-[#e0e0e0] bg-white p-2', className)}>
      <span className="text-[#888]">{label}</span>
      <span className={cn('text-[#333]', mono && 'font-mono text-[10px]', valueClassName)}>
        {displayValue}
      </span>
    </div>
  );
}
