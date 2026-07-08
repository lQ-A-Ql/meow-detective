import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

export function SectionHeader({
  icon: Icon,
  title,
  subtitle,
  className,
}: {
  icon?: LucideIcon;
  title: ReactNode;
  subtitle?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn('flex items-center gap-3 border-b border-[#eee] pb-2', className)}>
      {Icon ? <Icon size={18} className="text-[#555]" /> : null}
      <div>
        <div className="font-serif text-[15px] text-[#111]">{title}</div>
        {subtitle ? <div className="text-[11px] text-[#888]">{subtitle}</div> : null}
      </div>
    </div>
  );
}

export function PanelFrame({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <section className={cn('rounded border border-[#e0e0e0] bg-white p-4', className)}>
      {children}
    </section>
  );
}

export function EmptyState({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]', className)}>
      {children}
    </div>
  );
}
