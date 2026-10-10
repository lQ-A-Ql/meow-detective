import { cn } from '@/app/components/ui/utils';

export interface AnchorItem {
  id: string;
  label: string;
}

/**
 * Local navigation for a long form. Native anchors keep it keyboard-accessible and survive
 * without JavaScript; sections opt in by carrying a matching id and scroll-margin.
 */
export function AnchorNav({ items, label, className }: { items: AnchorItem[]; label: string; className?: string }) {
  return (
    <nav aria-label={label} className={cn('shrink-0', className)}>
      <ul className="space-y-1">
        {items.map((item) => (
          <li key={item.id}>
            <a
              href={'#' + item.id}
              className="block border-l-2 border-transparent px-3 py-1.5 text-[11px] text-forensics-muted transition-colors hover:border-forensics-primary-blue hover:bg-forensics-hover hover:text-forensics-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-forensics-primary-blue/40"
            >
              {item.label}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
