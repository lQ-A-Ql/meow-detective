import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';
import { isApiErrorDto } from '@/lib/api/client';

export function errorMessage(error: unknown) {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function StatCard({
  title,
  value,
  subtitle,
  icon: Icon,
}: {
  title: string;
  value: string | number;
  subtitle?: string;
  icon?: LucideIcon;
}) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white p-4">
      <div className="flex items-center justify-between">
        <div className="font-mono text-[11px] uppercase tracking-wider text-[#888]">{title}</div>
        {Icon ? <Icon size={16} className="text-[#aaa]" /> : null}
      </div>
      <div className="mt-1 font-mono text-2xl font-semibold text-[#111] truncate overflow-hidden">{value}</div>
      {subtitle ? <div className="mt-0.5 text-[11px] text-[#666]">{subtitle}</div> : null}
    </div>
  );
}

export function EmptyPlaceholder({ children }: { children: ReactNode }) {
  return (
    <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
      {children}
    </div>
  );
}

export function SectionHeader({
  icon: Icon,
  title,
  subtitle,
}: {
  icon: LucideIcon;
  title: string;
  subtitle?: string;
}) {
  return (
    <div className="flex items-center gap-3 border-b border-[#eee] pb-2">
      <Icon size={18} className="text-[#555]" />
      <div>
        <div className="font-serif text-[15px] text-[#111]">{title}</div>
        {subtitle ? <div className="text-[11px] text-[#888]">{subtitle}</div> : null}
      </div>
    </div>
  );
}
