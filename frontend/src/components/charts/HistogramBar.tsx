import { Button } from '@/app/components/ui/button';

/**
 * Chart hotspot primitive. A histogram bar is a control, so it renders through the shared button
 * primitive with the bar shape: it fills its slot, carries no text and exposes a range name.
 */
export function HistogramBar({
  heightPercent,
  filled,
  label,
  title,
  disabled,
  onSelect,
}: {
  heightPercent: number;
  filled: boolean;
  label: string;
  title: string;
  disabled: boolean;
  onSelect: () => void;
}) {
  return (
    <Button
      type="button"
      variant="viewerControl"
      shape="bar"
      title={title}
      aria-label={label}
      disabled={disabled}
      onClick={onSelect}
    >
      <span
        aria-hidden="true"
        className={`min-h-px w-full transition-colors ${filled ? 'bg-forensics-ink' : 'bg-forensics-line'}`}
        style={{ height: `${heightPercent}%` }}
      />
    </Button>
  );
}
