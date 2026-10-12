import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const inputVariants = cva(
  [
    "file:text-foreground placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground",
    "ui-line-field ui-control-motion flex w-full min-w-0 rounded-none bg-transparent focus:outline-none",
    "file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-light",
    "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50",
    "aria-invalid:text-destructive",
  ].join(" "),
  {
    variants: {
      variant: {
        line: "",
        default: "",
        forensics:
          "text-forensics-text",
        mono: "font-mono",
        path: "font-mono",
        search:
          "border-0 bg-transparent px-0 shadow-none focus-visible:border-transparent focus-visible:ring-0",
        numeric:
          "font-mono text-right",
      },
      inputSize: {
        line: "h-9 px-0 py-1 text-[12px]",
        default: "h-9 px-0 py-1 text-[12px]",
        compact: "h-7 px-0 py-0.5 text-[11px]",
        inline: "h-6 px-0 py-0.5 text-[11px]",
      },
    },
    defaultVariants: {
      variant: "default",
      inputSize: "default",
    },
  },
);

function Input({
  className,
  type,
  variant,
  inputSize,
  ...props
}: React.ComponentProps<"input"> & VariantProps<typeof inputVariants>) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(inputVariants({ variant, inputSize: inputSize ?? (variant === 'line' ? 'line' : undefined), className }))}
      {...props}
    />
  );
}

export { Input, inputVariants };
