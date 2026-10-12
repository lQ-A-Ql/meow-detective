import * as TooltipPrimitive from '@radix-ui/react-tooltip';
import type { ReactNode } from 'react';
import { cn } from './utils';

/**
 * The single tooltip primitive: trigger wiring, open delay, and the surface styling live
 * here so no view hand-rolls its own hover hint markup.
 */
export function Tooltip({
  content,
  children,
  side = 'right',
  sideOffset = 8,
  delayDuration = 150,
  className,
}: {
  content: ReactNode;
  children: ReactNode;
  side?: 'top' | 'right' | 'bottom' | 'left';
  sideOffset?: number;
  delayDuration?: number;
  className?: string;
}) {
  return (
    <TooltipPrimitive.Provider delayDuration={delayDuration}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            side={side}
            sideOffset={sideOffset}
            className={cn(
              'ui-overlay-surface ui-overlay-motion z-50 max-w-[min(360px,90vw)] rounded-ctl border border-forensics-border px-2.5 py-1.5 text-[11px] text-forensics-ink',
              className,
            )}
          >
            {content}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}
