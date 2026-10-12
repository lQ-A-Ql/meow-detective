import { PropsWithChildren, ReactNode } from 'react';
import { ScrollArea } from '@/app/components/ui/scroll-area';

interface InspectorPaneProps extends PropsWithChildren {
  title?: ReactNode;
  widthClassName?: string;
  subtitle?: ReactNode;
  className?: string;
  /** Short object-kind badge shown before the identity, e.g. file object / timeline event. */
  kind?: ReactNode;
  /** Pinned footer, so navigational actions never scroll away from the inspector. */
  actions?: ReactNode;
}

export function InspectorPane({ title, widthClassName = 'w-72', subtitle, kind, actions, className, children }: InspectorPaneProps) {
  return (
    <aside className={`${widthClassName} shrink-0 bg-transparent flex flex-col ${className ?? ''}`}>
      {title || subtitle ? (
        <div className="workspace-content shrink-0 bg-transparent">
          <div className="flex items-center gap-2">
            {kind ? <span className="border border-forensics-border-strong px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-forensics-muted">{kind}</span> : null}
            <div className="font-serif text-[11px] font-light tracking-wide text-forensics-text-tertiary">{title}</div>
          </div>
          {subtitle ? <div className="mt-1 text-[10px] font-mono text-forensics-muted-light truncate">{subtitle}</div> : null}
        </div>
      ) : null}
      <ScrollArea className="min-h-0 flex-1" viewportClassName="workspace-content">{children}</ScrollArea>
      {actions ? <div className="workspace-content shrink-0">{actions}</div> : null}
    </aside>
  );
}

export function InspectorSection({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="space-y-2 pt-4 first:pt-0">
      <div className="text-[11px] text-forensics-muted">{title}</div>
      <div>{children}</div>
    </section>
  );
}

export function InspectorValue({ value, mono, strong }: { value: ReactNode; mono?: boolean; strong?: boolean }) {
  return (
    <div
      className={[
        'break-all bg-transparent py-1 text-[12px]',
        mono ? 'font-mono' : 'font-sans',
        strong ? 'font-light text-forensics-text' : 'text-forensics-text-secondary',
      ].join(' ')}
    >
      {value}
    </div>
  );
}
