import { PropsWithChildren, ReactNode } from 'react';

interface InspectorPaneProps extends PropsWithChildren {
  title: ReactNode;
  widthClassName?: string;
  subtitle?: ReactNode;
}

export function InspectorPane({ title, widthClassName = 'w-72', subtitle, children }: InspectorPaneProps) {
  return (
    <aside className={`${widthClassName} shrink-0 border-l border-[#e0e0e0] bg-[#fafafa] flex flex-col`}>
      <div className="shrink-0 border-b border-[#e0e0e0] bg-[#f5f5f5] px-4 py-2">
        <div className="text-[10px] font-semibold tracking-wider text-[#555] uppercase">{title}</div>
        {subtitle ? <div className="mt-1 text-[10px] font-mono text-[#888]">{subtitle}</div> : null}
      </div>
      <div className="flex-1 overflow-auto p-4">{children}</div>
    </aside>
  );
}

export function InspectorSection({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="space-y-2 border-t border-[#e0e0e0] pt-4 first:border-t-0 first:pt-0">
      <div className="text-[10px] uppercase tracking-wide text-[#888]">{title}</div>
      <div>{children}</div>
    </section>
  );
}

export function InspectorValue({ value, mono, strong }: { value: ReactNode; mono?: boolean; strong?: boolean }) {
  return (
    <div
      className={[
        'break-all border border-[#ccc] bg-white p-2 text-[11px]',
        mono ? 'font-mono' : 'font-sans',
        strong ? 'font-medium text-[#111]' : 'text-[#333]',
      ].join(' ')}
    >
      {value}
    </div>
  );
}
