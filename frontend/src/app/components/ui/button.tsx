import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const buttonVariants = cva(
  "inline-flex cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-none bg-transparent text-sm font-light transition-colors duration-500 active:opacity-80 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 focus:outline-none focus-visible:border-forensics-primary-blue focus-visible:ring-1 focus-visible:ring-forensics-primary-blue/20 aria-invalid:border-destructive",
  {
    variants: {
      variant: {
        default: "border border-forensics-primary-blue text-forensics-primary-blue hover:bg-forensics-hover",
        destructive:
          "border border-destructive text-destructive hover:bg-forensics-error-bg focus-visible:ring-destructive/20",
        outline:
          "border border-forensics-border-strong text-foreground hover:border-forensics-sakura-500 hover:bg-forensics-hover",
        secondary:
          "border border-forensics-border text-secondary-foreground hover:bg-forensics-hover",
        ghost: "text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text",
        link: "text-primary underline-offset-4 hover:underline",
        forensicsPrimary:
          "border border-forensics-primary-blue text-forensics-primary-blue hover:bg-forensics-hover",
        forensicsOutline:
          "border border-forensics-border-strong text-forensics-text hover:border-forensics-sakura-500 hover:bg-forensics-hover",
        forensicsSurface:
          "border border-forensics-border text-forensics-text hover:border-forensics-sakura-500 hover:bg-forensics-hover",
        forensicsGhost:
          "text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text",
        forensicsDangerGhost:
          "text-forensics-muted-lighter hover:bg-forensics-error-bg hover:text-forensics-error-text",
        forensicsLink:
          "text-forensics-muted underline hover:text-forensics-text hover:no-underline",
        viewerControl:
          "text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text",
        mediaControl:
          "text-forensics-150 hover:bg-forensics-surface/10 hover:text-white",
        mediaPrimaryControl:
          "border border-forensics-150 text-forensics-150 hover:bg-forensics-surface/10",
        treeControl:
          "text-left text-forensics-text-tertiary hover:bg-forensics-hover data-[active=true]:bg-forensics-sakura-250 data-[active=true]:font-light data-[active=true]:text-forensics-text",
        canvasControl:
          "pointer-events-auto border border-forensics-border text-forensics-muted hover:border-forensics-sakura-500 hover:bg-forensics-hover hover:text-forensics-text",
        autocompleteOption:
          "rounded-none text-left font-mono hover:bg-forensics-highlight",
      },
      size: {
        default: "h-9 px-4 py-2 has-[>svg]:px-3",
        sm: "h-8 gap-1.5 px-3 has-[>svg]:px-2.5",
        lg: "h-10 px-6 has-[>svg]:px-4",
        icon: "size-9",
        xs: "h-7 gap-1.5 px-2 py-1 text-[11px] has-[>svg]:px-1.5",
        compact:
          "h-6 gap-1 px-2 py-0.5 text-[11px] has-[>svg]:px-1.5",
        iconXs: "size-5",
        iconSm: "size-6",
        viewerIcon: "size-7",
        mediaIcon: "size-7",
        mediaPrimary: "size-14",
        treeRow: "h-6 min-w-0 w-full justify-start gap-1 overflow-hidden px-2 py-1 text-[11px]",
        menuItem: "h-auto w-full justify-start gap-2 rounded-none px-3 py-1.5 text-[12px]",
        autocompleteItem: "h-auto w-full justify-start gap-2 rounded-none px-3 py-1.5 text-[12px]",
        canvasIcon: "size-7",
        inline: "h-auto p-0",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  }) {
  const Comp = asChild ? Slot : "button";

  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );
}

export { Button, buttonVariants };
