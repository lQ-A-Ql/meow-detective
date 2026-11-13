import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from './utils';

const buttonVariants = cva(
  "ui-control-motion ui-focus inline-flex cursor-pointer items-center justify-center border-0 bg-transparent active:opacity-80 disabled:pointer-events-none disabled:opacity-40 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none aria-invalid:text-destructive",
  {
    variants: {
      /** circle = icon action; pill = labeled action; row = full-width tree/list content. */
      shape: {
        circle: 'size-7 rounded-full',
        pill: 'min-h-8 w-fit max-w-full gap-2 rounded-full px-4 py-1.5 text-center text-[12px] leading-5 whitespace-normal',
        row: 'h-[22px] w-full justify-start gap-1.5 rounded-none px-2 text-left',
        /** Selectable surface in a grid or list: full-width card with a status edge. */
        card: 'h-auto w-full items-start justify-between gap-3 rounded-none border-l-2 border-l-transparent p-3 text-left',
        /** Chart hotspot: fills its slot, no padding, no text. */
        bar: 'h-full w-full cursor-crosshair items-end rounded-none p-0',
      },
      variant: {
        solid: 'bg-forensics-ink text-forensics-paper hover:bg-forensics-ink-2',
        default: 'bg-forensics-ink text-forensics-paper hover:bg-forensics-ink-2',
        destructive: 'text-destructive hover:bg-forensics-error-bg',
        outline: 'text-forensics-ink hover:bg-forensics-hover',
        secondary: 'bg-forensics-soft text-forensics-ink hover:bg-forensics-hover',
        ghost: 'text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text',
        link: 'text-primary underline-offset-4 hover:underline',
        forensicsPrimary: 'bg-forensics-ink text-forensics-paper hover:bg-forensics-ink-2',
        forensicsOutline: 'text-forensics-ink hover:bg-forensics-hover',
        forensicsSurface: 'text-forensics-ink hover:bg-forensics-hover',
        forensicsGhost: 'text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text',
        forensicsDangerGhost:
          'text-forensics-muted-lighter hover:bg-forensics-error-bg hover:text-forensics-error-text',
        forensicsLink: 'text-forensics-muted underline hover:text-forensics-text hover:no-underline',
        viewerControl: 'text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text',
        mediaControl: 'text-forensics-150 hover:bg-forensics-surface/10 hover:text-white',
        mediaPrimaryControl: 'text-forensics-150 hover:bg-forensics-surface/10',
        treeControl:
          'text-left text-forensics-text-tertiary hover:bg-forensics-hover data-[active=true]:bg-forensics-sakura-250 data-[active=true]:font-light data-[active=true]:text-forensics-text',
        canvasControl:
          'pointer-events-auto bg-forensics-paper text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text',
        autocompleteOption: 'rounded-none text-left font-mono hover:bg-forensics-highlight',
      },
      size: {
        // Width and height belong to the shape; labeled actions grow with their content.
        none: '',
        default: '',
        sm: '',
        xs: '',
        compact: '',
        lg: '',
        inline: '',
        icon: '',
        iconMd: '',
        iconSm: '',
        mediaIcon: '',
        mediaPrimary: '',
        treeRow: '',
        menuItem: '',
        autocompleteItem: '',
        canvasIcon: '',
        iconXs: 'size-6',
        viewerIcon: 'size-6',
      },
    },
    compoundVariants: [{ shape: 'circle', variant: ['default', 'solid', 'forensicsPrimary'], className: 'size-8' }],
    defaultVariants: {
      shape: 'circle',
      variant: 'default',
      size: 'default',
    },
  },
);

const Button = React.forwardRef<
  HTMLButtonElement,
  React.ComponentProps<'button'> &
    VariantProps<typeof buttonVariants> & {
      asChild?: boolean;
    }
>(function Button({ className, variant, size, shape, asChild = false, ...props }, ref) {
  const Comp = asChild ? Slot : 'button';

  return (
    <Comp
      ref={ref}
      data-slot="button"
      data-shape={shape ?? 'circle'}
      className={cn(buttonVariants({ variant, size, shape, className }))}
      {...props}
    />
  );
});
Button.displayName = 'Button';

export { Button, buttonVariants };
