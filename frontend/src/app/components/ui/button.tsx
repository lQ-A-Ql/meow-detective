import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const buttonVariants = cva(
  "inline-flex cursor-pointer items-center justify-center bg-transparent transition-colors duration-200 active:opacity-80 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-forensics-red aria-invalid:border-destructive",
  {
    variants: {
      /** circle = action button; row = full-width content row owned by tree/list primitives. */
      shape: {
        circle: "size-7 rounded-full",
        row: "h-[22px] w-full justify-start gap-1.5 rounded-none px-2 text-left",
      },
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
        none: "",
        // One circle size for every control: 28px. iconXs stays for the media control bar.
        default: "size-7",
        sm: "size-7",
        xs: "size-7",
        compact: "size-7",
        lg: "size-7",
        inline: "size-7",
        icon: "size-7",
        iconMd: "size-7",
        mediaIcon: "size-7",
        mediaPrimary: "size-7",
        treeRow: "size-7",
        menuItem: "size-7",
        autocompleteItem: "size-7",
        canvasIcon: "size-7",
        iconSm: "size-7",
        iconXs: "size-6",
        viewerIcon: "size-6",
      },
    },
    defaultVariants: {
      shape: "circle",
      variant: "default",
      size: "default",
    },
  },
);

const Button = React.forwardRef<
  HTMLButtonElement,
  React.ComponentProps<"button"> &
    VariantProps<typeof buttonVariants> & {
      asChild?: boolean;
    }
>(function Button({ className, variant, size, asChild = false, ...props }, ref) {
  const Comp = asChild ? Slot : "button";

  return (
    <Comp
      ref={ref}
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );
});
Button.displayName = "Button";

export { Button, buttonVariants };
