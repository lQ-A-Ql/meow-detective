"use client";

import * as React from "react";
import * as ProgressPrimitive from "@radix-ui/react-progress";

import { cn } from "./utils";

function Progress({
  className,
  value,
  indeterminate = false,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root> & { indeterminate?: boolean }) {
  const resolvedValue = indeterminate ? undefined : value;

  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      value={resolvedValue}
      className={cn(
        "bg-forensics-border-light relative h-1.5 w-full overflow-hidden rounded-none",
        className,
      )}
      {...props}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn(
          "bg-primary h-full w-full flex-1 transition-opacity duration-1000",
          indeterminate && "animate-pulse",
        )}
        style={indeterminate ? undefined : { transform: `translateX(-${100 - (value || 0)}%)` }}
      />
    </ProgressPrimitive.Root>
  );
}

export { Progress };
