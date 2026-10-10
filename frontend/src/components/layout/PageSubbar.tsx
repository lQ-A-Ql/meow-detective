import { PropsWithChildren } from 'react';

/** Page-level control strip. Page titles live nowhere in the shell, so only controls remain. */
export function PageSubbar({ children }: PropsWithChildren) {
  return (
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel">
      {children}
    </div>
  );
}
