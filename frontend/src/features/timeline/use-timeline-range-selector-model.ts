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
export function useTimelineRangeSelectorModel({
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
return { domainStart, domainEnd, startLabel, endLabel, rangeLabel, trackRef, valid, startRatio, endRatio, begin };
}
