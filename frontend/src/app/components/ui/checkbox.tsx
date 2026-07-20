"use client";

import * as React from "react";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { cva, type VariantProps } from "class-variance-authority";
import { CheckIcon } from "lucide-react";

import { cn } from "./utils";

const checkboxVariants = cva(
  [
    "peer shrink-0 rounded-none border bg-transparent transition-colors outline-none",
    "data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground data-[state=checked]:border-primary",
    "focus-visible:border-ring focus-visible:ring-ring/20 focus-visible:ring-1",
    "aria-invalid:ring-destructive/20 aria-invalid:border-destructive",
    "disabled:cursor-not-allowed disabled:opacity-50",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "",
        forensics:
          "border-forensics-border-strong data-[state=checked]:border-forensics-primary-blue data-[state=checked]:bg-forensics-primary-blue focus-visible:border-forensics-primary-blue focus-visible:ring-forensics-primary-blue/20",
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
