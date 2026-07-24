import { useState, type ReactNode } from 'react';
import * as CollapsiblePrimitive from '@radix-ui/react-collapsible';
import { ChevronDown, ChevronRight } from 'lucide-react';

export interface CollapsibleSectionProps {
  /** Header content rendered next to the collapse chevron. */
  title: ReactNode;
  /** Optional trailing content on the header row (e.g. counts, badges). */
  headerExtra?: ReactNode;
  defaultOpen?: boolean;
  className?: string;
  contentClassName?: string;
  children: ReactNode;
}

/**
 * Generic collapsible section primitive: a full-width header toggle with a
 * chevron and a collapsible body. Styling is intentionally minimal so feature
 * panels can compose their own borders/surfaces around it.
 */
export function CollapsibleSection({
  title,
  headerExtra,
  defaultOpen = true,
  className,
  contentClassName,
  children,
}: CollapsibleSectionProps) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <CollapsiblePrimitive.Root open={open} onOpenChange={setOpen} className={className}>
      <div className="flex items-center justify-between gap-3">
        <CollapsiblePrimitive.Trigger asChild>
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-2 text-left"
            aria-expanded={open}
          >
            {open ? (
              <ChevronDown size={14} className="shrink-0 text-forensics-muted" />
            ) : (
              <ChevronRight size={14} className="shrink-0 text-forensics-muted" />
            )}
            {title}
          </button>
        </CollapsiblePrimitive.Trigger>
        {headerExtra}
      </div>
      <CollapsiblePrimitive.Content className={contentClassName}>
        {children}
      </CollapsiblePrimitive.Content>
    </CollapsiblePrimitive.Root>
  );
}
