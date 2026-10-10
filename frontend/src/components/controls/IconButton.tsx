import type { ReactNode } from 'react';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { cn } from '@/app/components/ui/utils';

type IconButtonVariant =
  | 'forensicsGhost'
  | 'forensicsOutline'
  | 'forensicsSurface'
  | 'forensicsPrimary'
  | 'forensicsDangerGhost';

type IconButtonSize = 'iconXs' | 'iconSm' | 'icon';

/**
 * The single icon button in the product: a perfect circle holding exactly one SVG glyph. The
 * name never sits inside the circle - it lives in the accessible label and in the hover hint.
 */
export function IconButton({
  label,
  children,
  onClick,
  disabled = false,
  variant = 'forensicsGhost',
  size = 'iconSm',
  className,
}: {
  label: string;
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  variant?: IconButtonVariant;
  size?: IconButtonSize;
  className?: string;
}) {
  return (
    <Tooltip content={label}>
      <Button
        type="button"
        variant={variant}
        size={size}
        aria-label={label}
        onClick={onClick}
        disabled={disabled}
        className={cn('rounded-full', className)}
      >
        {children}
      </Button>
    </Tooltip>
  );
}
