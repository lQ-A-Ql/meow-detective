/**
 * HexViewer - 高性能十六进制查看器
 *
 * 优化点：
 * 1. 使用虚拟滚动只渲染可见行
 * 2. 使用 useMemo 缓存格式化结果
 * 3. 使用 useCallback 减少重渲染
 */

import { useMemo, useRef, useState, useCallback, useEffect } from 'react';

interface HexViewerProps {
  /** Hex 行数据 */
  lines: string[];
  /** 初始偏移量 */
  offset?: number;
  /** 行高 (px) */
  lineHeight?: number;
}

const DEFAULT_CONTAINER_HEIGHT = 600;
const OVERSCAN_ROWS = 5;
const LARGE_HEX_ROW_THRESHOLD = 1000;

/** 解析 hex 行为结构化数据 */
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

export function HexViewer({ lines, lineHeight = 20 }: HexViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(DEFAULT_CONTAINER_HEIGHT);

  // 监听容器大小变化
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

  // 计算可见范围
  const visibleRange = useMemo(() => {
    const startIndex = Math.max(0, Math.floor(scrollTop / lineHeight) - OVERSCAN_ROWS);
    const endIndex = Math.min(
      lines.length,
      Math.ceil((scrollTop + containerHeight) / lineHeight) + OVERSCAN_ROWS
    );
    return { startIndex, endIndex };
  }, [scrollTop, containerHeight, lineHeight, lines.length]);

  // 可见行
  const visibleLines = useMemo(() => {
    return lines
      .slice(visibleRange.startIndex, visibleRange.endIndex)
      .map((line, idx) => ({
        parsed: parseHexLine(line),
        lineIndex: visibleRange.startIndex + idx,
      }));
  }, [lines, visibleRange.endIndex, visibleRange.startIndex]);

  // 滚动处理
  const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(e.currentTarget.scrollTop);
  }, []);

  // 计算偏移量列宽
  const offsetWidth = useMemo(() => {
    if (lines.length === 0) return 80;
    const lastOffset = parseHexLine(lines[lines.length - 1] ?? '').offset || '00000000';
    return Math.max(80, lastOffset.length * 10 + 16);
  }, [lines]);

  const isLargeContent = lines.length >= LARGE_HEX_ROW_THRESHOLD;
  const visibleRowCount = visibleRange.endIndex - visibleRange.startIndex;

  return (
    <div className="flex h-full min-h-0 flex-col bg-white font-mono text-[11px]">
      {isLargeContent && (
        <div
          className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] px-3 py-1 text-[10px] text-[#666]"
          role="status"
        >
          大内容模式: 共 {lines.length.toLocaleString()} 行，仅在 DOM 中渲染可见附近的 {visibleRowCount.toLocaleString()} 行。
        </div>
      )}

      <div
        ref={containerRef}
        className="min-h-0 flex-1 overflow-auto"
        onScroll={handleScroll}
      >
        {/* 总高度占位 */}
        <div style={{ height: lines.length * lineHeight, position: 'relative' }}>
          {/* 可见行 */}
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
                  {/* 偏移量 */}
                  <div
                    className="shrink-0 text-[#999] text-right pr-2 select-none border-r border-[#eee] bg-[#fafafa]"
                    style={{ width: offsetWidth }}
                  >
                    {line.offset}
                  </div>

                  {/* Hex 字节 */}
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

                  {/* ASCII 预览 */}
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

        {/* 空状态 */}
        {lines.length === 0 && (
          <div className="flex h-full items-center justify-center text-[#999]">
            选择文件后显示十六进制预览
          </div>
        )}
      </div>
    </div>
  );
}
