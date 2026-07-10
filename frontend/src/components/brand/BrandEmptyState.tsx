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
        'relative isolate overflow-hidden rounded border border-dashed border-[#f4dce5] bg-[radial-gradient(circle_at_82%_12%,#fff0f4_0%,transparent_30%),linear-gradient(135deg,#fffdfd_0%,#ffffff_52%,#fff8fa_100%)] p-6 text-center shadow-[0_12px_40px_rgba(217,145,170,0.07)]',
        className,
      )}
    >
      <div className="pointer-events-none absolute -right-10 -top-10 h-40 w-40 rounded-full bg-[#fff0f4]/70 blur-2xl" />
      <div className="pointer-events-none absolute -bottom-16 left-8 h-36 w-36 rounded-full bg-[#fff6f8]/80 blur-3xl" />
      <BrandArt
        variant={variant}
        className={cn('relative mx-auto mb-3 h-32 w-32 drop-shadow-[0_14px_26px_rgba(120,50,70,0.18)]', artClassName)}
      />
      <div className="relative text-[15px] font-semibold text-[#111]">{title}</div>
      {description ? (
        <div className="relative mt-2 text-[12px] leading-6 text-[#666]">{description}</div>
      ) : null}
      {children ? <div className="relative mt-4">{children}</div> : null}
    </div>
  );
}
