import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';
import { EmptyState, MetricCard, SectionHeader as SharedSectionHeader } from '@/components/data-display';
import { errorMessage as formatErrorMessage } from '@/lib/errors';

export function errorMessage(error: unknown) {
  return formatErrorMessage(error);
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
  return <MetricCard label={title} value={value} subtitle={subtitle} icon={Icon} size="lg" />;
}

export function EmptyPlaceholder({ children }: { children: ReactNode }) {
  return <EmptyState className="mt-3">{children}</EmptyState>;
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
  return <SharedSectionHeader icon={Icon} title={title} subtitle={subtitle} />;
}
