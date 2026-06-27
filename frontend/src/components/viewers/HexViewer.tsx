import { useMemo, useRef, useState, useCallback, useEffect } from 'react';
import type { HexLoadedRange } from '@/types/models';

interface HexViewerProps {
  lines: string[];
  lineHeight?: number;
  activeOffset?: number;
  loadedRanges?: HexLoadedRange[];
  onNeedMoreRange?: (direction: 'previous' | 'next') => void;
}

const DEFAULT_CONTAINER_HEIGHT = 600;
const OVERSCAN_ROWS = 5;
interface ParsedLine {
  offset: string;
  hex: string;
  ascii: string;
}

function parseHexLine(line: string): ParsedLine {
  const parts = line.split(/\s{2,}/);
  if (parts.length >= 2) {
    const offset = parts[0];
    const hex = parts[1];
    // 生成 ASCII 预览
    const bytes = hex.split(' ').filter(h => h.length === 2);
    const ascii = bytes
      .map(h => {
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
  lineHeight = 20,
  activeOffset,
  loadedRanges,
  onNeedMoreRange,
}: HexViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(DEFAULT_CONTAINER_HEIGHT);

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
      lines.length,
      Math.ceil((scrollTop + containerHeight) / lineHeight) + OVERSCAN_ROWS
    );
    return { startIndex, endIndex };
  }, [scrollTop, containerHeight, lineHeight, lines.length]);

  const visibleLines = useMemo(() => {
    return lines
      .slice(visibleRange.startIndex, visibleRange.endIndex)
      .map((line, idx) => ({
        parsed: parseHexLine(line),
        lineIndex: visibleRange.startIndex + idx,
      }));
  }, [lines, visibleRange.endIndex, visibleRange.startIndex]);

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
    const firstVisibleOffset = parseHexLine(lines[0] ?? '').offset;
    const baseOffset = Number.parseInt(firstVisibleOffset || '0', 16) || 0;
    const lineIndex = Math.max(0, Math.floor((activeOffset - baseOffset) / 16));
    containerRef.current.scrollTop = lineIndex * lineHeight;
  }, [activeOffset, lineHeight, lines]);

  const offsetWidth = useMemo(() => {
    if (lines.length === 0) return 80;
    const lastOffset = parseHexLine(lines[lines.length - 1] ?? '').offset || '00000000';
    return Math.max(80, lastOffset.length * 10 + 16);
  }, [lines]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-white font-mono text-[11px]">
      <div
        ref={containerRef}
        className="min-h-0 flex-1 overflow-auto"
        onScroll={handleScroll}
      >
        <div style={{ height: lines.length * lineHeight, position: 'relative' }}>
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
                  className="flex hover:bg-[#f5f5f5]"
                  style={{ height: lineHeight }}
                >
                  <div
                    className="shrink-0 text-[#999] text-right pr-2 select-none border-r border-[#eee] bg-[#fafafa]"
                    style={{ width: offsetWidth }}
                  >
                    {line.offset}
                  </div>

                  <div className="flex-1 px-3 tracking-wider">
                    {line.hex.split(' ').map((byte, i) => (
                      <span key={i} className="inline-block w-[26px] text-center">
                        {byte === '00' ? (
                          <span className="text-[#ccc]">{byte}</span>
                        ) : byte === 'FF' ? (
                          <span className="text-[#e74c3c]">{byte}</span>
                        ) : (
                          <span className="text-[#333]">{byte}</span>
                        )}
                      </span>
                    ))}
                  </div>

                  <div className="shrink-0 w-[128px] pl-2 border-l border-[#eee] text-[#666]">
                    {line.ascii.split('').map((char, i) => (
                      <span
                        key={i}
                        className={
                          char === '.'
                            ? 'text-[#ccc]'
                            : char === ' '
                              ? 'text-[#999]'
                              : 'text-[#333]'
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

        {lines.length === 0 && (
          <div className="flex h-full items-center justify-center text-[#999]">
            选择文件后显示十六进制预览
          </div>
        )}
      </div>

      {loadedRanges?.length ? (
        <div className="shrink-0 border-t border-[#e0e0e0] bg-[#fafafa] px-3 py-1 text-[10px] text-[#777]">
          已加载区间: {loadedRanges.map((range) => `0x${range.start.toString(16).toUpperCase()}-0x${Math.max(range.start, range.end - 1).toString(16).toUpperCase()}`).join(', ')}
        </div>
      ) : null}
    </div>
  );
}
