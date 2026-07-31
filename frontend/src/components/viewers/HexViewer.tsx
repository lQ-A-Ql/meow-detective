import { useMemo, useRef, useState, useCallback, useEffect } from 'react';
import { HexByteRow } from '@/components/viewers/HexByteRow';
import {
  BYTES_PER_ROW,
  formatByteWindowLine,
  formatOffset,
  parseHexLine,
  type HexParsedLine,
} from '@/components/viewers/hex-viewer-model';
import type { HexByteWindowLines, HexLoadedRange } from '@/types/models';

interface HexViewerProps {
  lines: string[] | HexByteWindowLines;
  rawBytes?: number[];
  baseOffset?: number;
  fileSize?: number;
  lineHeight?: number;
  activeOffset?: number;
  loadedRanges?: HexLoadedRange[];
  onNeedMoreRange?: (direction: 'previous' | 'next') => void;
}

const DEFAULT_CONTAINER_HEIGHT = 600;
const OVERSCAN_ROWS = 5;

interface VisibleRange {
  startIndex: number;
  endIndex: number;
}

interface ByteSelection {
  anchor: number;
  focus: number;
}

function byteOffsetFromTarget(target: EventTarget | null): number | undefined {
  if (!(target instanceof Element)) return undefined;
  const cell = target.closest<HTMLElement>('[data-byte-offset]');
  if (!cell) return undefined;
  const offset = Number(cell.dataset.byteOffset);
  return Number.isSafeInteger(offset) && offset >= 0 ? offset : undefined;
}

function byteOffsetFromPointer(event: React.PointerEvent<HTMLDivElement>): number | undefined {
  const directOffset = byteOffsetFromTarget(event.target);
  if (directOffset !== undefined) return directOffset;
  if (typeof document.elementFromPoint !== 'function') return undefined;
  return byteOffsetFromTarget(document.elementFromPoint(event.clientX, event.clientY));
}

function visibleRangeFor(
  scrollTop: number,
  containerHeight: number,
  lineHeight: number,
  rowCount: number,
): VisibleRange {
  return {
    startIndex: Math.max(0, Math.floor(scrollTop / lineHeight) - OVERSCAN_ROWS),
    endIndex: Math.min(
      rowCount,
      Math.ceil((scrollTop + containerHeight) / lineHeight) + OVERSCAN_ROWS,
    ),
  };
}

export function HexViewer({
  lines,
  rawBytes,
  baseOffset,
  fileSize,
  lineHeight = 20,
  activeOffset,
  loadedRanges,
  onNeedMoreRange,
}: HexViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const dragAnchorRef = useRef<number>();
  const draggingRef = useRef(false);
  const [containerHeight, setContainerHeight] = useState(DEFAULT_CONTAINER_HEIGHT);
  const [hoveredOffset, setHoveredOffset] = useState<number>();
  const [selection, setSelection] = useState<ByteSelection>();

  const byteWindow = useMemo(() => {
    const metadata = lines as HexByteWindowLines;
    const bytes = rawBytes ?? metadata.rawBytes;
    return {
      rawBytes: bytes && bytes.length > 0 ? bytes : undefined,
      baseOffset: baseOffset ?? metadata.baseOffset ?? 0,
      fileSize: fileSize ?? metadata.fileSize,
    };
  }, [baseOffset, fileSize, lines, rawBytes]);

  const rowCount = byteWindow.rawBytes
    ? Math.ceil(byteWindow.rawBytes.length / BYTES_PER_ROW)
    : lines.length;

  const selectableRange = useMemo(() => {
    if (byteWindow.rawBytes?.length) {
      return {
        start: byteWindow.baseOffset,
        end: byteWindow.baseOffset + byteWindow.rawBytes.length - 1,
      };
    }
    let first: HexParsedLine | undefined;
    let last: HexParsedLine | undefined;
    for (let index = 0; index < lines.length && !first; index += 1) {
      const parsed = parseHexLine(lines[index]);
      if (parsed.bytes.length > 0) first = parsed;
    }
    for (let index = lines.length - 1; index >= 0 && !last; index -= 1) {
      const parsed = parseHexLine(lines[index]);
      if (parsed.bytes.length > 0) last = parsed;
    }
    if (!first || !last) return undefined;
    return {
      start: first.offsetValue,
      end: last.offsetValue + last.bytes.length - 1,
    };
  }, [byteWindow.baseOffset, byteWindow.rawBytes, lines]);

  const [visibleRange, setVisibleRange] = useState<VisibleRange>(() =>
    visibleRangeFor(0, DEFAULT_CONTAINER_HEIGHT, lineHeight, rowCount),
  );

  const updateVisibleRange = useCallback((scrollTop: number, viewportHeight: number) => {
    const nextRange = visibleRangeFor(scrollTop, viewportHeight, lineHeight, rowCount);
    setVisibleRange((currentRange) => (
      currentRange.startIndex === nextRange.startIndex && currentRange.endIndex === nextRange.endIndex
        ? currentRange
        : nextRange
    ));
  }, [lineHeight, rowCount]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const updateContainerHeight = () => {
      const nextHeight = container.clientHeight;
      if (nextHeight > 0) {
        setContainerHeight((currentHeight) => (
          currentHeight === nextHeight ? currentHeight : nextHeight
        ));
        updateVisibleRange(container.scrollTop, nextHeight);
      }
    };

    updateContainerHeight();

    if (typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const nextHeight = entry.contentRect.height;
        if (nextHeight > 0) {
          setContainerHeight((currentHeight) => (
            currentHeight === nextHeight ? currentHeight : nextHeight
          ));
          updateVisibleRange(container.scrollTop, nextHeight);
        }
      }
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, [updateVisibleRange]);

  useEffect(() => {
    const container = containerRef.current;
    updateVisibleRange(container?.scrollTop ?? 0, container?.clientHeight || containerHeight);
  }, [containerHeight, rowCount, updateVisibleRange]);

  const visibleLines = useMemo(() => {
    if (byteWindow.rawBytes) {
      return Array.from(
        { length: visibleRange.endIndex - visibleRange.startIndex },
        (_, index) => {
          const lineIndex = visibleRange.startIndex + index;
          return {
            parsed: formatByteWindowLine(byteWindow.rawBytes!, lineIndex, byteWindow.baseOffset),
            lineIndex,
          };
        },
      );
    }

    return lines
      .slice(visibleRange.startIndex, visibleRange.endIndex)
      .map((line, idx) => ({
        parsed: parseHexLine(line),
        lineIndex: visibleRange.startIndex + idx,
      }));
  }, [byteWindow, lines, visibleRange.endIndex, visibleRange.startIndex]);

  const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    const nextScrollTop = e.currentTarget.scrollTop;
    updateVisibleRange(nextScrollTop, e.currentTarget.clientHeight);

    if (!onNeedMoreRange) {
      return;
    }

    const maxScrollTop = Math.max(0, e.currentTarget.scrollHeight - e.currentTarget.clientHeight);
    if (nextScrollTop <= lineHeight * OVERSCAN_ROWS) {
      // A first chunk starts at zero. Do not duplicate the initial IPC range
      // read when a new viewport reports its initial scroll position.
      if (byteWindow.baseOffset > 0) {
        onNeedMoreRange('previous');
      }
    } else if (maxScrollTop - nextScrollTop <= lineHeight * (OVERSCAN_ROWS + 2)) {
      const hasMoreAfter =
        byteWindow.fileSize === undefined ||
        !byteWindow.rawBytes ||
        byteWindow.baseOffset + byteWindow.rawBytes.length < byteWindow.fileSize;
      if (hasMoreAfter) {
        onNeedMoreRange('next');
      }
    }
  }, [byteWindow, lineHeight, onNeedMoreRange, updateVisibleRange]);

  const handlePointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const offset = byteOffsetFromPointer(event);
    if (offset === undefined) return;
    if (draggingRef.current && dragAnchorRef.current !== undefined) {
      event.preventDefault();
      setSelection({ anchor: dragAnchorRef.current, focus: offset });
      return;
    }
    setHoveredOffset((current) => (current === offset ? current : offset));
  }, []);

  const handlePointerDown = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button > 0) return;
    const offset = byteOffsetFromTarget(event.target);
    if (offset === undefined) return;
    event.preventDefault();
    event.currentTarget.focus();
    const anchor = event.shiftKey && selection ? selection.anchor : offset;
    dragAnchorRef.current = anchor;
    draggingRef.current = true;
    setHoveredOffset(undefined);
    setSelection({ anchor, focus: offset });
  }, [selection]);

  const finishPointerSelection = useCallback(() => {
    draggingRef.current = false;
    dragAnchorRef.current = undefined;
  }, []);

  useEffect(() => {
    window.addEventListener('pointerup', finishPointerSelection);
    window.addEventListener('pointercancel', finishPointerSelection);
    window.addEventListener('blur', finishPointerSelection);
    return () => {
      window.removeEventListener('pointerup', finishPointerSelection);
      window.removeEventListener('pointercancel', finishPointerSelection);
      window.removeEventListener('blur', finishPointerSelection);
    };
  }, [finishPointerSelection]);

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!selectableRange) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      setSelection(undefined);
      return;
    }
    const current = selection?.focus
      ?? (activeOffset !== undefined
        && activeOffset >= selectableRange.start
        && activeOffset <= selectableRange.end
        ? activeOffset
        : selectableRange.start);
    let next = current;
    switch (event.key) {
      case 'ArrowLeft':
        next -= 1;
        break;
      case 'ArrowRight':
        next += 1;
        break;
      case 'ArrowUp':
        next -= BYTES_PER_ROW;
        break;
      case 'ArrowDown':
        next += BYTES_PER_ROW;
        break;
      case 'Home':
        next = current - ((current - selectableRange.start) % BYTES_PER_ROW);
        break;
      case 'End':
        next = current + (BYTES_PER_ROW - 1 - ((current - selectableRange.start) % BYTES_PER_ROW));
        break;
      default:
        return;
    }
    event.preventDefault();
    next = Math.min(selectableRange.end, Math.max(selectableRange.start, next));
    setHoveredOffset(undefined);
    setSelection({
      anchor: event.shiftKey ? selection?.anchor ?? current : next,
      focus: next,
    });

    const container = containerRef.current;
    if (!container) return;
    const rowIndex = Math.floor((next - selectableRange.start) / BYTES_PER_ROW);
    const rowTop = rowIndex * lineHeight;
    const rowBottom = rowTop + lineHeight;
    let nextScrollTop = container.scrollTop;
    if (rowTop < container.scrollTop) {
      nextScrollTop = rowTop;
    } else if (rowBottom > container.scrollTop + container.clientHeight) {
      nextScrollTop = rowBottom - container.clientHeight;
    }
    if (nextScrollTop !== container.scrollTop) {
      container.scrollTop = nextScrollTop;
      updateVisibleRange(nextScrollTop, container.clientHeight || containerHeight);
    }
  }, [activeOffset, containerHeight, lineHeight, selectableRange, selection, updateVisibleRange]);

  // In byte-window mode `lines` is only a compatibility wrapper and may be
  // recreated by unrelated view-model state changes. The bytes identify the
  // actual window whose selection must be retained.
  const selectionWindowSource = byteWindow.rawBytes ?? lines;

  useEffect(() => {
    draggingRef.current = false;
    dragAnchorRef.current = undefined;
    setHoveredOffset(undefined);
    setSelection(undefined);
  }, [byteWindow.baseOffset, selectionWindowSource]);

  useEffect(() => {
    if (activeOffset === undefined || !containerRef.current) {
      return;
    }
    const firstVisibleOffset = byteWindow.rawBytes
      ? byteWindow.baseOffset
      : Number.parseInt(parseHexLine(lines[0] ?? '').offset || '0', 16) || 0;
    const lineIndex = Math.min(
      Math.max(0, rowCount - 1),
      Math.max(0, Math.floor((activeOffset - firstVisibleOffset) / BYTES_PER_ROW)),
    );
    const nextScrollTop = lineIndex * lineHeight;
    containerRef.current.scrollTop = nextScrollTop;
    updateVisibleRange(nextScrollTop, containerRef.current.clientHeight || containerHeight);
  }, [activeOffset, byteWindow, containerHeight, lineHeight, lines, rowCount, updateVisibleRange]);

  const offsetWidth = useMemo(() => {
    if (rowCount === 0) return 80;
    const lastOffset = byteWindow.rawBytes
      ? formatOffset(
        Math.max(
          byteWindow.baseOffset + Math.max(0, byteWindow.rawBytes.length - 1),
          byteWindow.fileSize ? byteWindow.fileSize - 1 : 0,
        ),
      )
      : parseHexLine(lines[lines.length - 1] ?? '').offset || '00000000';
    return Math.max(80, lastOffset.length * 10 + 16);
  }, [byteWindow, lines, rowCount]);

  const selectionStart = selection ? Math.min(selection.anchor, selection.focus) : undefined;
  const selectionEnd = selection ? Math.max(selection.anchor, selection.focus) : undefined;

  return (
    <div className="flex h-full min-h-0 flex-col bg-forensics-surface font-mono text-[11px]">
      <div
        ref={containerRef}
        data-testid="hex-scroll-container"
        className="min-h-0 flex-1 touch-pan-y select-none overflow-auto"
        role="grid"
        tabIndex={rowCount > 0 ? 0 : -1}
        aria-label="Hex 与 ASCII 字节预览"
        onKeyDown={handleKeyDown}
        onPointerCancel={finishPointerSelection}
        onPointerDown={handlePointerDown}
        onPointerLeave={() => {
          if (!draggingRef.current) setHoveredOffset(undefined);
        }}
        onPointerMove={handlePointerMove}
        onPointerUp={finishPointerSelection}
        onScroll={handleScroll}
      >
        <div style={{ height: rowCount * lineHeight, position: 'relative' }}>
          <div
            data-testid="hex-visible-window"
            style={{
              position: 'absolute',
              top: visibleRange.startIndex * lineHeight,
              width: '100%',
            }}
          >
            {visibleLines.map(({ parsed: line, lineIndex }) => (
              <HexByteRow
                key={lineIndex}
                line={line}
                lineIndex={lineIndex}
                lineHeight={lineHeight}
                offsetWidth={offsetWidth}
                highlightedOffset={hoveredOffset}
                selectionStart={selectionStart}
                selectionEnd={selectionEnd}
                selectionFocus={selection?.focus}
              />
            ))}
          </div>
        </div>

        {rowCount === 0 && (
          <div className="flex h-full items-center justify-center text-forensics-muted-lighter">
            选择文件后显示十六进制预览
          </div>
        )}
      </div>

      {loadedRanges?.length ? (
        <div className="shrink-0 border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
          已加载区间: {loadedRanges.map((range) => `0x${range.start.toString(16).toUpperCase()}-0x${Math.max(range.start, range.end - 1).toString(16).toUpperCase()}`).join(', ')}
        </div>
      ) : null}
    </div>
  );
}
