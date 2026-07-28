import { PropsWithChildren, ReactNode } from 'react';
import { ScrollArea } from '@/app/components/ui/scroll-area';

interface InspectorPaneProps extends PropsWithChildren {
  title: ReactNode;
  widthClassName?: string;
  subtitle?: ReactNode;
  className?: string;
}

export function InspectorPane({ title, widthClassName = 'w-72', subtitle, className, children }: InspectorPaneProps) {
  return (
    <aside className={`${widthClassName} shrink-0 border-l border-forensics-border bg-forensics-panel flex flex-col ${className ?? ''}`}>
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel-strong px-5 py-3">
        <div className="font-serif text-[11px] font-light tracking-wide text-forensics-text-tertiary">{title}</div>
        {subtitle ? <div className="mt-1 text-[10px] font-mono text-forensics-muted-light truncate">{subtitle}</div> : null}
      </div>
      <ScrollArea className="min-h-0 flex-1" viewportClassName="p-5">{children}</ScrollArea>
    </aside>
  );
}

export function InspectorSection({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="space-y-2 border-t border-forensics-border pt-4 first:border-t-0 first:pt-0">
      <div className="font-serif text-[10px] tracking-wide text-forensics-muted-light">{title}</div>
      <div>{children}</div>
    </section>
  );
}

export function InspectorValue({ value, mono, strong }: { value: ReactNode; mono?: boolean; strong?: boolean }) {
  return (
    <div
      className={[
        'break-all border border-forensics-border-strong bg-transparent p-2 text-[11px]',
        mono ? 'font-mono' : 'font-sans',
        strong ? 'font-light text-forensics-text' : 'text-forensics-text-secondary',
      ].join(' ')}
    >
      {value}
    </div>
  );
}
