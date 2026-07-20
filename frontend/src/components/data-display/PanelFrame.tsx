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
      {Icon ? <Icon size={18} className="text-forensics-text-tertiary" /> : null}
      <div>
        <div className="font-serif text-[15px] font-light tracking-wide text-forensics-text">{title}</div>
        {subtitle ? <div className="text-[11px] text-forensics-muted-light">{subtitle}</div> : null}
      </div>
    </div>
  );
}

export function PanelFrame({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <section className={cn('rounded-none border border-forensics-border bg-transparent p-5 transition-colors', className)}>
      {children}
    </section>
  );
}

export function EmptyState({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('rounded-none border border-dashed border-forensics-border-strong bg-transparent p-6 text-center text-[12px] text-forensics-muted-light', className)}>
      {children}
    </div>
  );
}
