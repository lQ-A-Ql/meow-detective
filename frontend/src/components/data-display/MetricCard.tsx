import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

export interface MetricCardProps {
  label: ReactNode;
  value: ReactNode;
  subtitle?: ReactNode;
  icon?: LucideIcon;
  mono?: boolean;
  align?: 'left' | 'center';
  size?: 'sm' | 'md' | 'lg';
  className?: string;
}

export function MetricCard({
  label,
  value,
  subtitle,
  icon: Icon,
  mono = true,
  align = 'left',
  size = 'md',
  className,
}: MetricCardProps) {
  return (
    <div
      className={cn(
        'rounded border border-forensics-border bg-white shadow-[0_10px_28px_rgba(217,145,170,0.03)]',
        size === 'sm' && 'px-2 py-1.5',
        size === 'md' && 'p-3',
        size === 'lg' && 'p-4',
        align === 'center' && 'text-center',
        className,
      )}
    >
      <div className={cn('flex items-center justify-between gap-2', align === 'center' && 'justify-center')}>
        <div className="text-[10px] uppercase tracking-wider text-forensics-muted-light">{label}</div>
        {Icon ? <Icon size={16} className="text-[#aaa]" /> : null}
      </div>
      <div
        className={cn(
          'mt-1 break-words font-semibold text-[#111]',
          mono && 'font-mono',
          size === 'sm' && 'text-[11px]',
          size === 'md' && 'text-[15px]',
          size === 'lg' && 'text-2xl',
        )}
      >
        {value}
      </div>
      {subtitle ? <div className="mt-0.5 text-[11px] text-[#666]">{subtitle}</div> : null}
    </div>
  );
}

export function StatGrid({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('grid grid-cols-1 gap-4 md:grid-cols-3 xl:grid-cols-4', className)}>
      {children}
    </div>
  );
}
