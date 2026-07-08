import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const buttonVariants = cva(
  "inline-flex cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/90",
        destructive:
          "bg-destructive text-white hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60",
        outline:
          "border bg-background text-foreground hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ghost:
          "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
        link: "text-primary underline-offset-4 hover:underline",
        forensicsPrimary:
          "rounded-[2px] bg-forensics-text text-white hover:bg-forensics-text-secondary",
        forensicsOutline:
          "rounded-[2px] border border-forensics-border-strong bg-white text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text",
        forensicsSurface:
          "rounded-[2px] border border-forensics-border-strong bg-forensics-surface text-forensics-text hover:bg-forensics-hover",
        forensicsGhost:
          "rounded-[2px] text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text",
        forensicsDangerGhost:
          "rounded-[2px] text-forensics-muted-lighter hover:bg-red-50 hover:text-red-600",
        forensicsLink:
          "rounded-[2px] text-forensics-muted underline hover:text-forensics-text hover:no-underline",
        viewerControl:
          "rounded-[2px] text-forensics-muted hover:bg-forensics-hover hover:text-forensics-text",
        mediaControl:
          "rounded-[2px] text-[#999] hover:bg-white/10 hover:text-white",
        mediaPrimaryControl:
          "rounded-full bg-white text-black hover:bg-gray-200",
        treeControl:
          "rounded-none text-left text-[#555] hover:bg-[#eaeaea] data-[active=true]:bg-[#e0e8f0] data-[active=true]:font-medium data-[active=true]:text-[#111]",
        canvasControl:
          "pointer-events-auto rounded border border-forensics-border bg-white text-forensics-muted shadow-sm hover:bg-forensics-hover hover:text-forensics-text",
        autocompleteOption:
          "rounded-none text-left font-mono hover:bg-forensics-highlight",
      },
      size: {
        default: "h-9 px-4 py-2 has-[>svg]:px-3",
        sm: "h-8 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5",
        lg: "h-10 rounded-md px-6 has-[>svg]:px-4",
        icon: "size-9 rounded-md",
        xs: "h-7 gap-1.5 rounded-[2px] px-2 py-1 text-[11px] has-[>svg]:px-1.5",
        compact:
          "h-6 gap-1 rounded-[2px] px-2 py-0.5 text-[11px] has-[>svg]:px-1.5",
        iconXs: "size-5 rounded-[2px]",
        iconSm: "size-6 rounded-[2px]",
        viewerIcon: "size-7 rounded-[2px]",
        mediaIcon: "size-7 rounded-[2px]",
        mediaPrimary: "size-14 rounded-full",
        treeRow: "h-6 min-w-0 w-full justify-start gap-1 overflow-hidden px-2 py-1 text-[11px]",
        menuItem: "h-auto w-full justify-start gap-2 rounded-none px-3 py-1.5 text-[12px]",
        autocompleteItem: "h-auto w-full justify-start gap-2 rounded-none px-3 py-1.5 text-[12px]",
        canvasIcon: "size-7",
        inline: "h-auto rounded-[2px] p-0",
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
