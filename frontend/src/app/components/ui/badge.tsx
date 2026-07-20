import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const badgeVariants = cva(
  "inline-flex items-center justify-center rounded-none border px-2 py-0.5 text-xs font-light w-fit whitespace-nowrap shrink-0 [&>svg]:size-3 gap-1 [&>svg]:pointer-events-none focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/20 aria-invalid:ring-destructive/20 aria-invalid:border-destructive transition-colors overflow-hidden",
  {
    variants: {
      variant: {
        default:
          "border-forensics-primary-blue bg-transparent text-forensics-primary-blue [a&]:hover:bg-forensics-hover",
        secondary:
          "border-forensics-sakura-300 bg-transparent text-forensics-text [a&]:hover:bg-forensics-hover",
        destructive:
          "border-destructive bg-transparent text-destructive [a&]:hover:bg-forensics-error-bg focus-visible:ring-destructive/20",
        outline:
          "border-forensics-border text-forensics-text [a&]:hover:bg-forensics-hover [a&]:hover:text-forensics-text",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

function Badge({
  className,
  variant,
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot : "span";

  return (
    <Comp
      data-slot="badge"
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  );
}

export { Badge, badgeVariants };
