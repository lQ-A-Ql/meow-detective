import { ChevronDown, ChevronRight } from 'lucide-react';
import type { CSSProperties, ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

/**
 * The tree owns its primitives. A row is a treeitem, not an action button, and the expander is a
 * 16px in-row toggle instead of the product's 28px circular button, which does not belong inside
 * a 22px row.
 */
export function TreeToggle({
  expanded,
  hasChildren,
  label,
  onToggle,
}: {
  expanded: boolean;
  hasChildren: boolean;
  label: string;
  onToggle: () => void;
}) {
  if (!hasChildren) return <span aria-hidden="true" className="w-4 shrink-0" />;
  return (
    <span
      role="button"
      tabIndex={-1}
      aria-label={label}
      aria-expanded={expanded}
      data-tree-toggle="true"
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          event.stopPropagation();
          onToggle();
        }
      }}
      className="flex size-4 shrink-0 items-center justify-center text-forensics-ink-3 hover:text-forensics-ink"
    >
      {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
    </span>
  );
}

export function TreeRow({
  depth,
  active = false,
  expanded = false,
  hasChildren = false,
  selected,
  toggleLabel,
  ariaLabel,
  disabled = false,
  label,
  onToggle,
  onActivate,
  connector,
  icon,
  meta,
  className,
  style,
}: {
  depth: number;
  active?: boolean;
  expanded?: boolean;
  hasChildren?: boolean;
  selected?: boolean;
  /** Accessible name of the expander, e.g. "展开/折叠 目录名". */
  toggleLabel?: string;
  /** Accessible name of the row itself when the label is not plain text. */
  ariaLabel?: string;
  /** Rows are not form controls, so the state is exposed as aria-disabled. */
  disabled?: boolean;
  label: ReactNode;
  onToggle?: () => void;
  onActivate?: () => void;
  connector?: ReactNode;
  icon?: ReactNode;
  meta?: ReactNode;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <button
      type="button"
      role="treeitem"
      aria-label={ariaLabel}
      aria-disabled={disabled || undefined}
      aria-current={active ? 'true' : undefined}
      aria-selected={selected}
      aria-expanded={expanded}
      aria-level={depth + 1}
      tabIndex={0}
      data-active={active ? 'true' : undefined}
      disabled={disabled}
      onClick={onActivate}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault();
          onActivate?.();
        } else if (event.key === 'ArrowRight' && hasChildren && !expanded) {
          event.preventDefault();
          onToggle?.();
        } else if (event.key === 'ArrowLeft' && hasChildren && expanded) {
          event.preventDefault();
          onToggle?.();
        }
      }}
      className={cn(
        'relative flex h-[22px] w-full max-w-full items-center gap-1.5 pl-2 pr-2 text-left text-[12px] text-forensics-ink-2 hover:bg-forensics-hover',
        'data-[active=true]:bg-forensics-red-tint data-[active=true]:text-forensics-red-ink',
        disabled && 'pointer-events-none opacity-50',
        className,
      )}
      style={style}
    >
      {connector}
      {onToggle || hasChildren ? (
        <TreeToggle
          expanded={expanded}
          hasChildren={hasChildren}
          label={toggleLabel ?? ''}
          onToggle={() => onToggle?.()}
        />
      ) : null}
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {meta}
    </button>
  );
}
