import { Children, isValidElement } from 'react';
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
        'min-w-0 bg-transparent',
        size === 'sm' && 'px-3 py-1.5',
        size === 'md' && 'px-3 py-2',
        size === 'lg' && 'px-4 py-3',
        align === 'center' && 'text-center',
        className,
      )}
    >
      <div className={cn('flex items-center justify-between gap-2', align === 'center' && 'justify-center')}>
        <div className="text-[10px] tracking-[0.08em] text-forensics-ink-3">{label}</div>
        {Icon ? <Icon size={14} className="text-forensics-ink-3" /> : null}
      </div>
      <div
        className={cn(
          'mt-0.5 break-words font-light text-forensics-ink',
          mono && 'font-mono',
          size === 'sm' && 'text-[11px]',
          size === 'md' && 'text-[15px]',
          size === 'lg' && 'text-2xl',
        )}
      >
        {value}
      </div>
      {subtitle ? <div className="mt-0.5 text-[11px] text-forensics-ink-3">{subtitle}</div> : null}
    </div>
  );
}

export function StatGrid({
  children,
  className,
  collapseZeros = false,
  hiddenNote,
}: {
  children: ReactNode;
  className?: string;
  /**
   * Hide zero-valued tiles when the grid also carries real values, and say how many were
   * hidden. A grid where everything is zero still renders every tile, so a board section can
   * never become an empty shell.
   */
  collapseZeros?: boolean;
  hiddenNote?: (hidden: number) => ReactNode;
}) {
  const items = Children.toArray(children);
  const valueOf = (node: ReactNode) => (isValidElement(node) ? (node.props as { value?: ReactNode }).value : undefined);
  const isZero = (value: ReactNode) => value === 0 || value === '0';
  const nonZero = items.filter((node) => !isZero(valueOf(node)));
  const collapse = collapseZeros && nonZero.length > 0;
  const rendered = collapse ? nonZero : items;
  const hidden = items.length - rendered.length;

  return (
    <div
      className={cn(
        // 指标行：区块内的竖金线分隔，不再使用卡片
        'flex flex-wrap items-stretch [&>*+*]:border-l [&>*+*]:border-forensics-divider',
        className,
      )}
    >
      {rendered}
      {hidden > 0 && hiddenNote ? (
        <div className="w-full px-3 py-1 text-[10px] text-forensics-ink-3">{hiddenNote(hidden)}</div>
      ) : null}
    </div>
  );
}
