import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const inputVariants = cva(
  [
    "file:text-foreground placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground",
    "border-input flex w-full min-w-0 rounded-none border bg-transparent transition-colors duration-500 focus:outline-none",
    "file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-light",
    "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50",
    "focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/20",
    "aria-invalid:ring-destructive/20 aria-invalid:border-destructive",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "",
        forensics:
          "border-forensics-border-strong text-forensics-text focus-visible:border-forensics-sakura-500 focus-visible:ring-1 focus-visible:ring-forensics-sakura-400/20",
        mono: "border-forensics-border-strong font-mono text-forensics-text focus-visible:border-forensics-sakura-500 focus-visible:ring-1 focus-visible:ring-forensics-sakura-400/20",
        path: "border-forensics-border-strong font-mono text-forensics-text focus-visible:border-forensics-sakura-500 focus-visible:ring-1 focus-visible:ring-forensics-sakura-400/20",
        search:
          "border-0 bg-transparent px-0 shadow-none focus-visible:border-transparent focus-visible:ring-0",
        numeric:
          "border-forensics-border-strong font-mono text-right text-forensics-text focus-visible:border-forensics-sakura-500 focus-visible:ring-1 focus-visible:ring-forensics-sakura-400/20",
      },
      inputSize: {
        default: "h-9 px-3 py-1 text-base md:text-sm",
        compact: "h-7 px-2 py-0.5 text-[11px]",
        inline: "h-6 px-1.5 py-0.5 text-[11px]",
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
      className={cn(inputVariants({ variant, inputSize, className }))}
      {...props}
    />
  );
}

export { Input, inputVariants };
