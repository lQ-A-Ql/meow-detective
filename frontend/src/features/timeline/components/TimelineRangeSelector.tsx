import { useEffect, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

interface TimelineRangeSelectorProps {
  domainStart?: string;
  domainEnd?: string;
  selectedStart?: string;
  selectedEnd?: string;
  onChange: (start: string, end: string) => void;
  startLabel: string;
  endLabel: string;
  rangeLabel: string;
}

type DragMode = 'start' | 'end' | 'window';

function timestamp(value?: string) {
  if (!value) return undefined;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function iso(value: number) {
  return new Date(value).toISOString();
}

export function TimelineRangeSelector({
  domainStart,
  domainEnd,
  selectedStart,
  selectedEnd,
  onChange,
  startLabel,
  endLabel,
  rangeLabel,
}: TimelineRangeSelectorProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const localRangeRef = useRef<{ start: number; end: number }>();
  const [drag, setDrag] = useState<{ mode: DragMode; anchor: number; start: number; end: number }>();
  const [localRange, setLocalRange] = useState<{ start: number; end: number }>();
  const domainStartMs = timestamp(domainStart);
  const domainEndMs = timestamp(domainEnd);
  const selectedStartMs = localRange?.start ?? timestamp(selectedStart) ?? domainStartMs;
  const selectedEndMs = localRange?.end ?? timestamp(selectedEnd) ?? domainEndMs;
  const valid = domainStartMs !== undefined && domainEndMs !== undefined && domainEndMs > domainStartMs;
  const startRatio = valid && selectedStartMs !== undefined
    ? Math.max(0, Math.min(1, (selectedStartMs - domainStartMs) / (domainEndMs - domainStartMs)))
    : 0;
  const endRatio = valid && selectedEndMs !== undefined
    ? Math.max(0, Math.min(1, (selectedEndMs - domainStartMs) / (domainEndMs - domainStartMs)))
    : 1;

  useEffect(() => {
    if (!drag) return undefined;
    const move = (event: globalThis.PointerEvent) => {
      const rect = trackRef.current?.getBoundingClientRect();
      if (!rect || !valid || domainStartMs === undefined || domainEndMs === undefined) return;
      const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
      const delta = (ratio - drag.anchor) * (domainEndMs - domainStartMs);
      let nextStart = drag.start;
      let nextEnd = drag.end;
      if (drag.mode === 'start') nextStart = Math.min(drag.end - 1, Math.max(domainStartMs, domainStartMs + ratio * (domainEndMs - domainStartMs)));
      if (drag.mode === 'end') nextEnd = Math.max(drag.start + 1, Math.min(domainEndMs, domainStartMs + ratio * (domainEndMs - domainStartMs)));
      if (drag.mode === 'window') {
        const length = drag.end - drag.start;
        nextStart = Math.max(domainStartMs, Math.min(domainEndMs - length, drag.start + delta));
        nextEnd = nextStart + length;
      }
      localRangeRef.current = { start: nextStart, end: nextEnd };
      setLocalRange(localRangeRef.current);
    };
    const end = () => {
      const range = localRangeRef.current;
      if (range) onChange(iso(range.start), iso(range.end));
      localRangeRef.current = undefined;
      setLocalRange(undefined);
      setDrag(undefined);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end, { once: true });
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
    };
  }, [domainEndMs, domainStartMs, drag, onChange, valid]);

  function begin(mode: DragMode, event: ReactPointerEvent) {
    if (!valid || domainStartMs === undefined || domainEndMs === undefined || selectedStartMs === undefined || selectedEndMs === undefined) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect) return;
    setDrag({
      mode,
      anchor: (event.clientX - rect.left) / rect.width,
      start: selectedStartMs,
      end: selectedEndMs,
    });
    localRangeRef.current = { start: selectedStartMs, end: selectedEndMs };
    setLocalRange({ start: selectedStartMs, end: selectedEndMs });
  }

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
