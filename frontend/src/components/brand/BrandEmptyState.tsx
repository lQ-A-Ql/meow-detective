import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';
import { BrandWatermark } from './BrandWatermark';

export function BrandEmptyState({
  title,
  description,
  children,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        'relative isolate overflow-hidden rounded-none border border-dashed border-forensics-border-strong bg-transparent p-6 text-center transition-colors',
        className,
      )}
    >
      <BrandWatermark
        motif="sitting"
        className="absolute -bottom-12 -right-2 h-36 opacity-[0.055]"
      />
      <div className="relative font-serif text-[15px] font-light tracking-wide text-forensics-text">{title}</div>
      {description ? (
        <div className="relative mt-2 text-[12px] leading-6 text-forensics-muted">{description}</div>
      ) : null}
      {children ? <div className="relative mt-4">{children}</div> : null}
    </div>
  );
}
