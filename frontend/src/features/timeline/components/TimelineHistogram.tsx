import { useTranslation } from 'react-i18next';
import { HistogramBar } from '@/components/charts/HistogramBar';


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
            <HistogramBar
              heightPercent={bar.height}
              filled={bar.count > 0}
              label={t('timeline.histogram.selectRange', { count: bar.count })}
              title={t('timeline.histogram.barTitle', { range: rangeLabel, count: bar.count })}
              disabled={!hasRange}
              onSelect={() => onSelectRange(bar)}
            />
          </div>
        );
      })}
    </div>
  );
}
