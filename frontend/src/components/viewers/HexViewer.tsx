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
  const [containerHeight, setContainerHeight] = useState(600);

  // 监听容器大小变化
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerHeight(entry.contentRect.height);
      }
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // 解析所有行
  const parsedLines = useMemo(() => {
    return lines.map(parseHexLine);
  }, [lines]);

  // 计算可见范围
  const visibleRange = useMemo(() => {
    const startIndex = Math.max(0, Math.floor(scrollTop / lineHeight) - 5);
    const endIndex = Math.min(
      parsedLines.length,
      Math.ceil((scrollTop + containerHeight) / lineHeight) + 5
    );
    return { startIndex, endIndex };
  }, [scrollTop, containerHeight, lineHeight, parsedLines.length]);

  // 可见行
  const visibleLines = useMemo(() => {
    return parsedLines.slice(visibleRange.startIndex, visibleRange.endIndex);
  }, [parsedLines, visibleRange]);

  // 滚动处理
  const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(e.currentTarget.scrollTop);
  }, []);

  // 计算偏移量列宽
  const offsetWidth = useMemo(() => {
    if (parsedLines.length === 0) return 80;
    const lastOffset = parsedLines[parsedLines.length - 1]?.offset || '00000000';
    return Math.max(80, lastOffset.length * 10 + 16);
  }, [parsedLines]);

  return (
    <div
      ref={containerRef}
      className="h-full overflow-auto font-mono text-[11px] bg-white"
      onScroll={handleScroll}
    >
      {/* 总高度占位 */}
      <div style={{ height: parsedLines.length * lineHeight, position: 'relative' }}>
        {/* 可见行 */}
        <div
          style={{
            position: 'absolute',
            top: visibleRange.startIndex * lineHeight,
            width: '100%',
          }}
        >
          {visibleLines.map((line, idx) => {
            const lineIndex = visibleRange.startIndex + idx;
            return (
              <div
                key={lineIndex}
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
      {parsedLines.length === 0 && (
        <div className="flex items-center justify-center h-full text-[#999]">
          选择文件后显示十六进制预览
        </div>
      )}
    </div>
  );
}
