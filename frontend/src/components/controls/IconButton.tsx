import { forwardRef, type ComponentPropsWithoutRef, type ReactNode } from 'react';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { cn } from '@/app/components/ui/utils';

type IconButtonVariant =
  | 'solid'
  | 'destructive'
  | 'forensicsGhost'
  | 'forensicsOutline'
  | 'forensicsSurface'
  | 'forensicsPrimary'
  | 'forensicsDangerGhost';

type IconButtonSize = 'iconXs' | 'iconSm' | 'icon' | 'iconMd';

/**
 * The single icon button in the product: a perfect circle holding exactly one SVG glyph. The
 * name never sits inside the circle - it lives in the accessible label and in the hover hint.
 */
interface IconButtonProps extends Omit<ComponentPropsWithoutRef<typeof Button>, 'aria-label' | 'size' | 'variant' | 'shape'> {
  label: string;
  children: ReactNode;
  pressed?: boolean;
  variant?: IconButtonVariant;
  size?: IconButtonSize;
  testId?: string;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton({
  label,
  // data-testid stays opt-in so the control keeps a single, name-based query surface.

  children,
  onClick,
  disabled = false,
  pressed,
  variant = 'forensicsGhost',
  size = 'iconSm',
  className,
  testId,
  ...props
}, ref) {
  return (
    <Tooltip content={label}>
      <Button
        ref={ref}
        type="button"
        {...props}
        variant={variant}
        size={size}
        aria-label={label}
        onClick={onClick}
        disabled={disabled}
        aria-pressed={pressed}
        data-testid={testId}
        className={cn('rounded-full', className)}
      >
        {children}
      </Button>
    </Tooltip>
  );
});
