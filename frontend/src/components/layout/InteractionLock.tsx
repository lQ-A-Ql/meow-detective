import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

interface InteractionLockProps {
  locked: boolean;
  children: ReactNode;
  className?: string;
}

/** Keeps a layout region visible while preventing pointer, focus, and keyboard input. */
export function InteractionLock({ locked, children, className }: InteractionLockProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    if (locked) {
      container.setAttribute('inert', '');
      return;
    }
    container.removeAttribute('inert');
  }, [locked]);

  return (
    <div
      ref={containerRef}
      className={cn(className, locked && 'pointer-events-none select-none')}
      aria-busy={locked}
    >
      {children}
    </div>
  );
}
