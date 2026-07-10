"use client";

import * as React from "react";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { cva, type VariantProps } from "class-variance-authority";
import { CheckIcon } from "lucide-react";

import { cn } from "./utils";

const checkboxVariants = cva(
  [
    "peer shrink-0 rounded-[4px] border bg-input-background shadow-xs transition-shadow outline-none",
    "data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground data-[state=checked]:border-primary",
    "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
    "aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:bg-input/30 dark:data-[state=checked]:bg-primary",
    "disabled:cursor-not-allowed disabled:opacity-50",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "",
        forensics:
          "border-forensics-border-strong bg-white data-[state=checked]:border-forensics-sakura-500 data-[state=checked]:bg-forensics-sakura-500 focus-visible:border-forensics-sakura-500 focus-visible:ring-forensics-sakura-400/20",
      },
      checkboxSize: {
        default: "size-4",
        compact: "size-3.5",
      },
    },
    defaultVariants: {
      variant: "default",
      checkboxSize: "default",
    },
  },
);

function Checkbox({
  className,
  variant,
  checkboxSize,
  ...props
}: React.ComponentProps<typeof CheckboxPrimitive.Root> &
  VariantProps<typeof checkboxVariants>) {
  return (
    <CheckboxPrimitive.Root
      data-slot="checkbox"
      className={cn(checkboxVariants({ variant, checkboxSize, className }))}
      {...props}
    >
      <CheckboxPrimitive.Indicator
        data-slot="checkbox-indicator"
        className="flex items-center justify-center text-current transition-none"
      >
        <CheckIcon className="size-3.5" />
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}

export { Checkbox, checkboxVariants };
