import { Badge, type badgeVariants } from '@/app/components/ui/badge';
import type { VariantProps } from 'class-variance-authority';

type StatusBadgeProps = {
  label: string;
  variant?: NonNullable<VariantProps<typeof badgeVariants>>['variant'];
};

export function StatusBadge({ label, variant = 'outline' }: StatusBadgeProps) {
  return <Badge variant={variant}>{label}</Badge>;
}
