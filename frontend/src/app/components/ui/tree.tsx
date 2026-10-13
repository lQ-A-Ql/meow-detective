import { forwardRef, type CSSProperties, type HTMLAttributes, type ReactNode } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Button } from './button';
import { cn } from './utils';

export const Tree = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(function Tree({ className, ...props }, ref) {
  return <div {...props} ref={ref} role="tree" className={cn('ui-tree', className)} />;
});

export function TreeGroup(props: HTMLAttributes<HTMLDivElement>) {
  return <div {...props} role="group" />;
}

export function TreeToggle({ expanded, hasChildren, label, onToggle }: {
  expanded: boolean; hasChildren: boolean; label: string; onToggle: () => void;
}) {
  if (!hasChildren) return <span aria-hidden="true" className="w-4 shrink-0" />;
  return <span role="button" tabIndex={-1} aria-label={label} aria-expanded={expanded}
    data-tree-toggle="true" className="ui-tree-toggle"
    onClick={(event) => { event.stopPropagation(); onToggle(); }}
    onKeyDown={(event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault(); event.stopPropagation(); onToggle();
      }
    }}><ActionIcon name={expanded ? 'chevron-down' : 'chevron-right'} className="size-3" /></span>;
}

export interface TreeRowProps {
  depth: number; active?: boolean; expanded?: boolean; hasChildren?: boolean; selected?: boolean;
  toggleLabel?: string; ariaLabel?: string; disabled?: boolean; label: ReactNode;
  onToggle?: () => void; onActivate?: () => void; connector?: ReactNode; icon?: ReactNode;
  meta?: ReactNode; className?: string; style?: CSSProperties;
  tabIndex?: number; id?: string; positionInSet?: number; setSize?: number;
  onFocus?: () => void; compositeNavigation?: boolean; ownedGroupId?: string;
}

export function TreeRow({ depth, active = false, expanded = false, hasChildren = false,
  selected, toggleLabel, ariaLabel, disabled = false, label, onToggle, onActivate, connector,
  icon, meta, className, style, tabIndex = 0, id, positionInSet, setSize, onFocus,
  compositeNavigation = false, ownedGroupId,
}: TreeRowProps) {
  return <Button type="button" variant="forensicsGhost" shape="row" role="treeitem"
    id={id} aria-label={ariaLabel} aria-disabled={disabled || undefined}
    aria-current={active ? 'true' : undefined} aria-selected={selected}
    aria-expanded={hasChildren ? expanded : undefined} aria-level={depth + 1}
    aria-posinset={positionInSet} aria-setsize={setSize} tabIndex={tabIndex} aria-owns={ownedGroupId}
    data-active={active ? 'true' : undefined} disabled={disabled} onClick={onActivate} onFocus={onFocus}
    onKeyDown={(event) => {
      if (compositeNavigation) return;
      if (event.key === 'Enter') { event.preventDefault(); onActivate?.(); }
      else if (event.key === 'ArrowRight' && hasChildren && !expanded) { event.preventDefault(); onToggle?.(); }
      else if (event.key === 'ArrowLeft' && hasChildren && expanded) { event.preventDefault(); onToggle?.(); }
    }} className={cn('ui-tree-row', disabled && 'pointer-events-none opacity-50', className)} style={style}>
    {connector}
    {onToggle || hasChildren ? <TreeToggle expanded={expanded} hasChildren={hasChildren}
      label={toggleLabel ?? ''} onToggle={() => onToggle?.()} /> : null}
    {icon}<span className="min-w-0 flex-1 truncate">{label}</span>{meta}
  </Button>;
}
