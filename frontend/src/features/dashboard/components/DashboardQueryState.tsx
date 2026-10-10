import type { ReactNode } from 'react';
import { EmptyState } from '@/components/data-display';
import { useTranslation } from 'react-i18next';
import { errorMessage } from '@/lib/errors';

interface DashboardQueryStateProps {
  /** Next step shown when the section has nothing to display yet. */
  emptyAction?: ReactNode;
  isLoading?: boolean;
  isError?: boolean;
  error?: unknown;
  hasData: boolean;
  emptyMessage?: string;
  children: ReactNode;
}

export function DashboardQueryState({
  emptyAction,
  isLoading = false,
  isError = false,
  error,
  hasData,
  emptyMessage,
  children,
}: DashboardQueryStateProps) {
  const { t } = useTranslation();
  const resolvedEmptyMessage = emptyMessage ?? t('dashboard.query.empty');
  if (isLoading) {
    return <div className="mt-3 rounded-none border border-forensics-border bg-forensics-panel p-4 text-[12px] text-forensics-muted">{t('dashboard.query.loading')}</div>;
  }
  if (isError) {
    return <div className="mt-3 rounded-none border border-forensics-error-border bg-forensics-error-bg p-4 text-[12px] text-forensics-error-text">{errorMessage(error)}</div>;
  }
  if (!hasData) {
    return <EmptyState className="mt-3" title={resolvedEmptyMessage} action={emptyAction} />;
  }
  return <>{children}</>;
}
