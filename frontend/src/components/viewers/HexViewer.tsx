import { useMemo, useRef, useState, useCallback, useEffect } from 'react';
import { ScrollArea } from '@/app/components/ui/scroll-area';
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
const BYTES_PER_ROW = 16;

interface ParsedLine {
  offset: string;
  hex: string;
  bytes?: string[];
  ascii: string;
}

function formatOffset(offset: number) {
  return offset.toString(16).toUpperCase().padStart(8, '0');
}

function formatByte(byte: number) {
  return byte.toString(16).toUpperCase().padStart(2, '0');
}

function formatAsciiByte(byte: number) {
  return byte >= 32 && byte <= 126 ? String.fromCharCode(byte) : '.';
}

function formatByteWindowLine(rawBytes: number[], rowIndex: number, baseOffset: number): ParsedLine {
  const rowStart = rowIndex * BYTES_PER_ROW;
  const rowBytes = rawBytes.slice(rowStart, rowStart + BYTES_PER_ROW);
  const bytes = rowBytes.map(formatByte);
  return {
    offset: formatOffset(baseOffset + rowStart),
    hex: bytes.join(' '),
    bytes,
    ascii: rowBytes.map(formatAsciiByte).join(''),
  };
}

function parseHexLine(line: string): ParsedLine {
  const parts = line.split(/\s{2,}/);
  if (parts.length >= 2) {
    const offset = parts[0];
    const hex = parts[1];
    const bytes = hex.split(' ').filter((h) => h.length === 2);
    const ascii = bytes
      .map((h) => {
        const code = parseInt(h, 16);
        return code >= 32 && code <= 126 ? String.fromCharCode(code) : '.';
      })
      .join('');
    return { offset, hex, ascii };
  }
  return { offset: '', hex: line, ascii: '' };
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
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(DEFAULT_CONTAINER_HEIGHT);

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

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const updateContainerHeight = () => {
      const nextHeight = container.clientHeight;
      if (nextHeight > 0) {
        setContainerHeight(nextHeight);
      }
    };

    updateContainerHeight();

    if (typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerHeight(entry.contentRect.height);
      }
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const visibleRange = useMemo(() => {
    const startIndex = Math.max(0, Math.floor(scrollTop / lineHeight) - OVERSCAN_ROWS);
    const endIndex = Math.min(
      rowCount,
      Math.ceil((scrollTop + containerHeight) / lineHeight) + OVERSCAN_ROWS
    );
    return { startIndex, endIndex };
  }, [scrollTop, containerHeight, lineHeight, rowCount]);

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
    setScrollTop(nextScrollTop);

    if (!onNeedMoreRange) {
      return;
    }

    const maxScrollTop = Math.max(0, e.currentTarget.scrollHeight - e.currentTarget.clientHeight);
    if (nextScrollTop <= lineHeight * OVERSCAN_ROWS) {
      onNeedMoreRange('previous');
    } else if (maxScrollTop - nextScrollTop <= lineHeight * (OVERSCAN_ROWS + 2)) {
      onNeedMoreRange('next');
    }
  }, [lineHeight, onNeedMoreRange]);

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
    setScrollTop(nextScrollTop);
  }, [activeOffset, byteWindow, lineHeight, lines, rowCount]);

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

  return (
    <div className="flex h-full min-h-0 flex-col bg-forensics-surface font-mono text-[11px]">
      <ScrollArea
        className="min-h-0 flex-1"
        viewportRef={containerRef}
        viewportTestId="hex-scroll-container"
        viewportProps={{ onScroll: handleScroll }}
        showHorizontalScrollbar
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
            {visibleLines.map(({ parsed: line, lineIndex }) => {
              return (
                <div
                  key={lineIndex}
                  data-row-index={lineIndex}
                  className="flex hover:bg-forensics-panel-strong"
                  style={{ height: lineHeight }}
                >
                  <div
                    className="shrink-0 border-r border-forensics-border-light bg-forensics-panel pr-2 text-right text-forensics-muted-lighter select-none"
                    style={{ width: offsetWidth }}
                  >
                    {line.offset}
                  </div>

                  <div className="flex-1 min-w-0 grid grid-cols-[repeat(16,minmax(min-content,1fr))] gap-0 px-3 tracking-wider">
                    {(line.bytes ?? line.hex.split(' ')).map((byte, i) => (
                      <span key={i} className="text-center">
                        {byte === '00' ? (
                          <span className="text-forensics-muted-lighter">{byte}</span>
                        ) : byte === 'FF' ? (
                          <span className="text-forensics-error-text">{byte}</span>
                        ) : (
                          <span className="text-forensics-text-secondary">{byte}</span>
                        )}
                      </span>
                    ))}
                  </div>

                  <div className="shrink-0 min-w-[6rem] w-[8rem] border-l border-forensics-border-light pl-2 text-forensics-muted grid grid-cols-[repeat(16,minmax(min-content,1fr))] gap-0">
                    {line.ascii.split('').map((char, i) => (
                      <span
                        key={i}
                        className={
                          char === '.'
                            ? 'text-forensics-muted-lighter'
                            : char === ' '
                              ? 'text-forensics-muted-lighter'
                              : 'text-forensics-text-secondary'
                        }
                      >
                        {char}
                      </span>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {rowCount === 0 && (
          <div className="flex h-full items-center justify-center text-forensics-muted-lighter">
            选择文件后显示十六进制预览
          </div>
        )}
      </ScrollArea>

      {loadedRanges?.length ? (
        <div className="shrink-0 border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
          已加载区间: {loadedRanges.map((range) => `0x${range.start.toString(16).toUpperCase()}-0x${Math.max(range.start, range.end - 1).toString(16).toUpperCase()}`).join(', ')}
        </div>
      ) : null}
    </div>
  );
}
