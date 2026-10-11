import { useTranslation } from 'react-i18next';

export interface TimelineHistogramBar {
  count: number;
  height: number;
  startTs?: string;
  endTs?: string;
}

interface TimelineHistogramProps {
  bars: TimelineHistogramBar[];
  onSelectRange: (bar: TimelineHistogramBar) => void;
}

/** Event-density plot. It owns its hit targets instead of using Button styling. */
export function TimelineHistogram({ bars, onSelectRange }: TimelineHistogramProps) {
  const { t } = useTranslation();
  return (
    <div className="relative flex h-16 min-h-16 w-full items-end gap-px overflow-hidden rounded-ctl bg-forensics-panel-strong px-2 pt-2" aria-label={t('timeline.histogram.label')}>
      <div className="pointer-events-none absolute inset-x-2 top-1/3 border-t border-forensics-line/60" />
      <div className="pointer-events-none absolute inset-x-2 top-2/3 border-t border-forensics-line/60" />
      <div className="pointer-events-none absolute inset-x-2 bottom-0 border-t border-forensics-line-2" />
      {bars.map((bar, index) => {
        const hasRange = Boolean(bar.startTs && bar.endTs);
        const rangeLabel = hasRange ? `${bar.startTs} - ${bar.endTs}` : t('timeline.histogram.noRange');
        return (
          <div
            key={hasRange ? JSON.stringify([bar.startTs, bar.endTs]) : `empty-${index}`}
            role="button"
            tabIndex={hasRange ? 0 : -1}
            aria-disabled={!hasRange}
            aria-label={t('timeline.histogram.selectRange', { count: bar.count })}
            title={t('timeline.histogram.barTitle', { range: rangeLabel, count: bar.count })}
            className={`group relative z-[1] h-full min-w-0 flex-1 ${hasRange ? 'cursor-pointer' : 'cursor-default'}`}
            onClick={() => { if (hasRange) onSelectRange(bar); }}
            onKeyDown={(event) => {
              if (hasRange && (event.key === 'Enter' || event.key === ' ')) {
                event.preventDefault();
                onSelectRange(bar);
              }
            }}
          >
            <span
              aria-hidden="true"
              className={`absolute bottom-0 left-px right-px min-h-px rounded-t-[2px] transition-[height,background-color] duration-150 ${bar.count > 0 ? 'bg-forensics-red-ink/65 group-hover:bg-forensics-red-ink' : 'bg-forensics-line-2/60'}`}
              style={{ height: `${Math.max(bar.count > 0 ? 3 : 1, bar.height)}%` }}
            />
          </div>
        );
      })}
    </div>
  );
}
