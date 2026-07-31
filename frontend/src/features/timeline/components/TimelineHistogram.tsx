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
  return (
    <div className="flex flex-1 items-end gap-[1px] px-2">
      {bars.map((bar, index) => {
        const hasRange = Boolean(bar.startTs && bar.endTs);
        const rangeLabel = hasRange ? `${bar.startTs} - ${bar.endTs}` : '无时间区间';
        return (
          <div key={`${bar.startTs ?? 'empty'}-${index}`} className="h-full min-w-0 flex-1">
            <Button
              type="button"
              variant="viewerControl"
              size="inline"
              className="group h-full w-full cursor-crosshair items-end p-0"
              title={`${rangeLabel}：${bar.count} 条事件`}
              aria-label={`筛选该时间区间，共 ${bar.count} 条事件`}
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
