import * as React from 'react';
import * as ProgressPrimitive from '@radix-ui/react-progress';

import { cn } from './utils';

function Progress({
  className,
  value,
  max = 100,
  indeterminate = false,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root> & { indeterminate?: boolean }) {
  const resolvedValue = indeterminate ? null : (value ?? null);
  const fraction = Math.min(1, Math.max(0, (resolvedValue ?? 0) / max));

  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      value={resolvedValue}
      max={max}
      className={cn(
        'ui-progress bg-forensics-border-light relative h-1 w-full overflow-hidden rounded-full',
        className,
      )}
      {...props}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className="ui-progress-indicator bg-forensics-ink h-full w-full origin-left"
        style={resolvedValue === null ? undefined : { transform: `scaleX(${fraction})` }}
      />
    </ProgressPrimitive.Root>
  );
}

export { Progress };
