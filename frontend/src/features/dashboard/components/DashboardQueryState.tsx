import type { ReactNode } from 'react';
import { errorMessage } from '@/lib/errors';

interface DashboardQueryStateProps {
  isLoading?: boolean;
  isError?: boolean;
  error?: unknown;
  hasData: boolean;
  emptyMessage?: string;
  children: ReactNode;
}

export function DashboardQueryState({
  isLoading = false,
  isError = false,
  error,
  hasData,
  emptyMessage = '暂无数据。',
  children,
}: DashboardQueryStateProps) {
  if (isLoading) {
    return <div className="mt-3 rounded-none border border-forensics-border bg-forensics-panel p-4 text-[12px] text-forensics-muted">正在加载...</div>;
  }
  if (isError) {
    return <div className="mt-3 rounded-none border border-forensics-error-border bg-forensics-error-bg p-4 text-[12px] text-forensics-error-text">{errorMessage(error)}</div>;
  }
  if (!hasData) {
    return <div className="mt-3 rounded-none border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-lighter">{emptyMessage}</div>;
  }
  return <>{children}</>;
}
