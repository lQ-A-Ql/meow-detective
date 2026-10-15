import type { useTimelineRangeSelectorModel } from '../use-timeline-range-selector-model';


export function TimelineRangeSelectorView({ model }: { model: ReturnType<typeof useTimelineRangeSelectorModel> }) {
const { domainStart, domainEnd, startLabel, endLabel, rangeLabel, trackRef, valid, startRatio, endRatio, begin } = model;



  return (
    <div className="px-3 pb-0 pt-0.5" aria-label={rangeLabel}>
      <div ref={trackRef} className={`relative h-6 select-none touch-none ${valid ? 'cursor-ew-resize' : 'opacity-40'}`}>
        <div className="absolute inset-x-0 top-2.5 h-[6px] rounded-[2px] border border-forensics-line-2 bg-forensics-panel-strong" />
        {valid ? (
          <>
            <div
              className="absolute top-[3px] h-[7px] border-y border-forensics-red-ink/40 bg-forensics-red-tint"
              style={{ left: `${startRatio * 100}%`, width: `${Math.max(0, (endRatio - startRatio) * 100)}%` }}
              onPointerDown={(event) => begin('window', event)}
              aria-label={rangeLabel}
            />
            <div role="slider" tabIndex={0} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(startRatio * 100)} aria-label={startLabel} className="group absolute top-0 z-10 h-6 w-3 -translate-x-1/2 cursor-ew-resize touch-none outline-none focus-visible:ring-2 focus-visible:ring-forensics-red" style={{ left: `${startRatio * 100}%` }} onPointerDown={(event) => begin('start', event)}><span className="absolute left-1/2 top-0 h-6 w-px -translate-x-1/2 bg-forensics-red-ink group-hover:w-0.5" /></div>
            <div role="slider" tabIndex={0} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(endRatio * 100)} aria-label={endLabel} className="group absolute top-0 z-10 h-6 w-3 -translate-x-1/2 cursor-ew-resize touch-none outline-none focus-visible:ring-2 focus-visible:ring-forensics-red" style={{ left: `${endRatio * 100}%` }} onPointerDown={(event) => begin('end', event)}><span className="absolute left-1/2 top-0 h-6 w-px -translate-x-1/2 bg-forensics-red-ink group-hover:w-0.5" /></div>
          </>
        ) : null}
      </div>
      <div className="flex justify-between font-mono text-[9px] text-forensics-muted-light">
        <span>{domainStart ? new Date(domainStart).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '-'}</span>
        <span>{domainEnd ? new Date(domainEnd).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '-'}</span>
      </div>
    </div>
  );
}
