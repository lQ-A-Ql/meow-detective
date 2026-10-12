import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type UIEvent } from 'react';
import { TEXT_DEFAULT_VIEWPORT_HEIGHT, TEXT_OVERSCAN_LINES, TEXT_ROW_HEIGHT } from '@/design/text-viewer';
import { rowForOffset } from './text-pages';
import type { TextPageLine, TextScrollRequest } from './types';

export function useTextViewerViewport(lines: readonly TextPageLine[], request: TextScrollRequest, gutterWidth: number) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const pendingHorizontal = useRef<TextScrollRequest>();
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(TEXT_DEFAULT_VIEWPORT_HEIGHT);
  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const measure = () => { if (element.clientHeight > 0) setHeight(element.clientHeight); };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const range = useMemo(() => {
    const start = Math.max(0, Math.min(lines.length - 1, Math.floor(scrollTop / TEXT_ROW_HEIGHT)) - TEXT_OVERSCAN_LINES);
    return { start, end: Math.min(lines.length, start + Math.ceil(height / TEXT_ROW_HEIGHT) + TEXT_OVERSCAN_LINES * 2) };
  }, [scrollTop, height, lines.length]);

  // Phase 1 moves only this viewport and schedules the required virtual rows.
  useLayoutEffect(() => {
    const element = viewportRef.current;
    if (!element || request.kind === 'keep') return;
    pendingHorizontal.current = request.kind === 'match' ? request : undefined;
    const row = request.kind === 'match' ? rowForOffset(lines, request.startOffset ?? 0) : 0;
    const target = request.kind === 'match' ? row * TEXT_ROW_HEIGHT - (height - TEXT_ROW_HEIGHT) / 2 : 0;
    const next = Math.max(0, Math.min(target, lines.length * TEXT_ROW_HEIGHT - height));
    element.scrollTop = next;
    if (request.kind === 'top') element.scrollLeft = 0;
    setScrollTop(next);
  }, [request, lines, height]);

  // Phase 2 uses the actual glyph rectangle, so tabs, CJK and browser zoom do not shift the target.
  useLayoutEffect(() => {
    const element = viewportRef.current;
    const pending = pendingHorizontal.current;
    if (!element || !pending || pending !== request) return;
    const mark = element.querySelector<HTMLElement>(`mark[data-match-index="${pending.matchIndex}"][data-current="true"]`);
    if (!mark) return;
    const bounds = element.getBoundingClientRect();
    const hit = mark.getBoundingClientRect();
    const available = element.clientWidth - gutterWidth;
    if (available > 0) {
      const left = bounds.left + gutterWidth;
      const delta = hit.width >= available
        ? hit.left - left - 8
        : hit.left + hit.width / 2 - (left + available / 2);
      element.scrollLeft = Math.max(0, element.scrollLeft + delta);
    }
    pendingHorizontal.current = undefined;
  }, [request, range.start, range.end, gutterWidth, lines]);

  const onScroll = useCallback((event: UIEvent<HTMLDivElement>) => setScrollTop(event.currentTarget.scrollTop), []);
  return { viewportRef, onScroll, range, visibleLines: lines.slice(range.start, range.end) };
}
