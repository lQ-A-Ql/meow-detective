import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';

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

export function TimelineHistogram({ bars, onSelectRange }: TimelineHistogramProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 items-end gap-[1px] px-2">
      {bars.map((bar, index) => {
        const hasRange = Boolean(bar.startTs && bar.endTs);
        const rangeLabel = hasRange ? `${bar.startTs} - ${bar.endTs}` : t('timeline.histogram.noRange');
        return (
          <div key={hasRange ? JSON.stringify([bar.startTs, bar.endTs]) : `empty-${index}`} className="h-full min-w-0 flex-1">
            <Button
              type="button"
              variant="viewerControl"
              size="inline"
              className="group h-full w-full cursor-crosshair items-end p-0"
              title={t('timeline.histogram.barTitle', { range: rangeLabel, count: bar.count })}
              aria-label={t('timeline.histogram.selectRange', { count: bar.count })}
              disabled={!hasRange}
              onClick={() => onSelectRange(bar)}
            >
              <span
                aria-hidden="true"
                className={`min-h-px w-full transition-colors group-hover:bg-forensics-sakura-500 ${
                  bar.count > 0 ? 'bg-forensics-text' : 'bg-forensics-250'
                }`}
                style={{ height: `${bar.height}%` }}
              />
            </Button>
          </div>
        );
      })}
    </div>
  );
}
