import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const badgeVariants = cva(
  "ui-control-motion inline-flex items-center justify-center rounded-badge border-0 px-1.5 py-0.5 text-[10px] font-light w-fit whitespace-nowrap shrink-0 [&>svg]:size-3 gap-1 [&>svg]:pointer-events-none overflow-hidden",
  {
    variants: {
      variant: {
        default:
          "bg-forensics-soft text-forensics-ink [a&]:hover:bg-forensics-hover",
        secondary:
          "bg-transparent text-forensics-muted [a&]:hover:bg-forensics-hover",
        destructive:
          "bg-forensics-error-bg text-destructive",
        outline:
          "text-forensics-muted [a&]:hover:bg-forensics-hover [a&]:hover:text-forensics-text",
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
