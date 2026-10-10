import type { CSSProperties, ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';

/**
 * A list owns its rows. Rows are native buttons styled as rows - full width, left aligned, with
 * icon/label/meta slots - because the product's circular action button is for actions, not for
 * carrying content.
 */
export function ListRow({
  selected = false,
  disabled = false,
  icon,
  label,
  meta,
  onActivate,
  ariaLabel,
  ariaCurrent,
  className,
  style,
  testId,
}: {
  selected?: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  label: ReactNode;
  meta?: ReactNode;
  onActivate?: () => void;
  ariaLabel?: string;
  ariaCurrent?: boolean;
  className?: string;
  style?: CSSProperties;
  testId?: string;
}) {
  return (
    <button
      type="button"
      aria-label={ariaLabel}
      aria-current={ariaCurrent ? 'true' : undefined}
      aria-selected={selected}
      data-selected={selected ? 'true' : undefined}
      data-testid={testId}
      disabled={disabled}
      onClick={onActivate}
      className={cn(
        'flex w-full min-w-0 items-center gap-2 px-2 text-left text-[12px] text-forensics-ink-2 hover:bg-forensics-hover',
        'data-[selected=true]:bg-forensics-red-tint data-[selected=true]:text-forensics-red-ink',
        disabled && 'pointer-events-none opacity-50',
        className,
      )}
      style={style}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {meta}
    </button>
  );
}
