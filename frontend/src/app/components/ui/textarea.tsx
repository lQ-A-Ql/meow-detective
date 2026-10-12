import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const textareaVariants = cva(
  [
    "ui-line-field ui-control-motion placeholder:text-muted-foreground flex field-sizing-content w-full min-w-0 bg-transparent scrollbar-none",
    "focus:outline-none disabled:cursor-not-allowed disabled:opacity-40",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "resize-none",
        forensics:
          "resize-y",
        mono:
          "resize-y font-mono bg-forensics-soft",
      },
      textareaSize: {
        default: "min-h-16 px-0 py-2 text-[12px]",
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
