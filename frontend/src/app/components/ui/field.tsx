import * as React from "react";

import { cn } from "./utils";

function Field({ className, layout = 'stack', ...props }: React.ComponentProps<"div"> & { layout?: 'stack' | 'row' }) {
  return (
    <div
      data-slot="field"
      className={cn(layout === 'row' ? 'ui-form-row' : 'space-y-1.5', className)}
      {...props}
    />
  );
}

function FieldRow({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="field-row"
      className={cn("flex items-center gap-2", className)}
      {...props}
    />
  );
}

function FieldLabel({ className, ...props }: React.ComponentProps<"label">) {
  return (
    <label
      data-slot="field-label"
      className={cn("block text-[12px] font-light text-forensics-text-secondary", className)}
      {...props}
    />
  );
}

function FieldHint({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="field-hint"
      className={cn("text-[11px] leading-5 text-forensics-muted", className)}
      {...props}
    />
  );
}

function FieldError({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="field-error"
      className={cn("text-[11px] leading-5 text-forensics-error-text", className)}
      {...props}
    />
  );
}

export { Field, FieldError, FieldHint, FieldLabel, FieldRow };
