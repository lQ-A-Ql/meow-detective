import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';
import { BrandArt, type BrandArtVariant } from './BrandArt';

export function BrandEmptyState({
  variant = 'investigate',
  title,
  description,
  children,
  className,
  artClassName,
}: {
  variant?: BrandArtVariant;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  className?: string;
  artClassName?: string;
}) {
  return (
    <div
      className={cn(
        'relative isolate overflow-hidden rounded-none border border-dashed border-forensics-border-strong bg-transparent p-6 text-center transition-colors',
        className,
      )}
    >
      <BrandArt
        variant={variant}
        className={cn('relative mx-auto mb-3 h-32 w-32', artClassName)}
      />
      <div className="relative font-serif text-[15px] font-light tracking-wide text-forensics-text">{title}</div>
      {description ? (
        <div className="relative mt-2 text-[12px] leading-6 text-forensics-muted">{description}</div>
      ) : null}
      {children ? <div className="relative mt-4">{children}</div> : null}
    </div>
  );
}
