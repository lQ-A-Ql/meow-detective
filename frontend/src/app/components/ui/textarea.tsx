import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const textareaVariants = cva(
  [
    "border-input placeholder:text-muted-foreground flex field-sizing-content w-full rounded-none border bg-transparent",
    "transition-colors duration-500 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50",
    "focus-visible:border-ring focus-visible:ring-ring/20 focus-visible:ring-1",
    "aria-invalid:ring-destructive/20 aria-invalid:border-destructive",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "resize-none",
        forensics:
          "resize-y border-forensics-border-strong text-forensics-text focus-visible:border-forensics-text focus-visible:ring-0",
        mono:
          "resize-y border-forensics-border-strong font-mono text-forensics-text focus-visible:border-forensics-text focus-visible:ring-0",
      },
      textareaSize: {
        default: "min-h-16 px-3 py-2 text-base md:text-sm",
        compact: "min-h-14 px-2 py-1.5 text-[12px]",
        inline: "min-h-8 px-2 py-1 text-[11px]",
      },
    },
    defaultVariants: {
      variant: "default",
      textareaSize: "default",
    },
  },
);

function Textarea({
  className,
  variant,
  textareaSize,
  unstyled = false,
  ...props
}: React.ComponentProps<"textarea"> & VariantProps<typeof textareaVariants> & { unstyled?: boolean }) {
  return (
    <textarea
      data-slot="textarea"
      className={unstyled ? className : cn(textareaVariants({ variant, textareaSize, className }))}
      {...props}
    />
  );
}

export { Textarea, textareaVariants };
