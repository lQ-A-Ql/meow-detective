import { PropsWithChildren, ReactNode } from 'react';

interface PageSubbarProps extends PropsWithChildren {
  title?: ReactNode;
  meta?: ReactNode;
}

export function PageSubbar({ title, meta, children }: PageSubbarProps) {
  return (
    <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa]">
      {title || meta ? (
        <div className="flex min-h-8 items-center justify-between gap-4 border-b border-[#ececec] px-4 py-1.5 text-[10px] uppercase tracking-wider text-[#666]">
          <div className="font-semibold text-[#555]">{title}</div>
          <div className="font-mono text-[#888]">{meta}</div>
        </div>
      ) : null}
      {children}
    </div>
  );
}
