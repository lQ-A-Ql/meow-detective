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
    <div className={cn('flex items-center gap-3 border-b border-forensics-border-light pb-2', className)}>
      {Icon ? <Icon size={18} className="text-[#555]" /> : null}
      <div>
        <div className="font-serif text-[15px] text-[#111]">{title}</div>
        {subtitle ? <div className="text-[11px] text-forensics-muted-light">{subtitle}</div> : null}
      </div>
    </div>
  );
}

export function PanelFrame({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <section className={cn('rounded border border-forensics-border bg-white p-4 shadow-[0_12px_34px_rgba(217,145,170,0.03)]', className)}>
      {children}
    </section>
  );
}

export function EmptyState({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('rounded border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-light', className)}>
      {children}
    </div>
  );
}
