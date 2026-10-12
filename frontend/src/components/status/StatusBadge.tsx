import { Badge, type badgeVariants } from '@/app/components/ui/badge';
import type { VariantProps } from 'class-variance-authority';

type StatusBadgeProps = {
  label: string;
  variant?: NonNullable<VariantProps<typeof badgeVariants>>['variant'];
  tone?: 'neutral' | 'success' | 'error';
};

export function StatusBadge({ label, variant = 'outline', tone }: StatusBadgeProps) {
  const color = tone === 'success' ? 'text-forensics-success-text' : tone === 'error' ? 'text-forensics-error-text' : 'text-forensics-muted';
  return <Badge variant={variant} className={tone ? color : undefined}>{tone ? <span aria-hidden="true" className="size-1.5 shrink-0 rounded-full bg-current" /> : null}{label}</Badge>;
}
