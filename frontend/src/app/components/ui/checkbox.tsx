"use client";

import * as React from "react";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { cva, type VariantProps } from "class-variance-authority";
import { CheckIcon } from "lucide-react";

import { cn } from "./utils";

const checkboxVariants = cva(
  [
    "ui-control-motion ui-focus peer shrink-0 cursor-pointer rounded-badge border border-forensics-border-strong bg-transparent outline-none",
    "data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground data-[state=checked]:border-primary",
    "aria-invalid:ring-destructive/20 aria-invalid:border-destructive",
    "disabled:cursor-not-allowed disabled:opacity-50",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "",
        forensics:
          "data-[state=checked]:border-forensics-ink data-[state=checked]:bg-forensics-ink",
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

const Checkbox = React.forwardRef<
  React.ElementRef<typeof CheckboxPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof CheckboxPrimitive.Root> & VariantProps<typeof checkboxVariants>
>(function Checkbox({
  className,
  variant,
  checkboxSize,
  ...props
}, ref) {
  return (
    <CheckboxPrimitive.Root
      ref={ref}
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
});

export { Checkbox, checkboxVariants };
