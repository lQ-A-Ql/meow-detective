/**
 * TextViewer - 文本预览组件
 *
 * 功能：
 * - 行号显示
 * - 编码检测与切换
 * - 大文件分页
 * - 搜索高亮
 * - 语法高亮（可选）
 */

import { useState, useMemo, useRef, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronLeft, ChevronRight, FileText, Search } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';

interface TextViewerProps {
  /** 文本内容 */
  content: string;
  /** 文件编码 */
  encoding: string;
  /** 文件扩展名 (用于语法高亮) */
  extension?: string;
  /** 搜索关键词 */
  searchQuery?: string;
  /** 是否截断 */
  isTruncated?: boolean;
}

const PAGE_SIZE = 1000;
const ROW_HEIGHT = 18;
const OVERSCAN_LINES = 8;
const DEFAULT_CONTAINER_HEIGHT = 600;
const LARGE_TEXT_LINE_THRESHOLD = 1000;
const MAX_RENDER_SEGMENT_LENGTH = 8 * 1024;

interface TextPageLine {
  content: string;
  lineNumber: number;
  continuation: boolean;
}

interface TextPage {
  lines: TextPageLine[];
  logicalLineCount: number;
  nextStartOffset?: number;
}

function appendLineSegments(
  lines: TextPageLine[],
  content: string,
  startOffset: number,
  endOffset: number,
  lineNumber: number,
) {
  if (startOffset === endOffset) {
    lines.push({ content: '', lineNumber, continuation: false });
    return;
  }

  let segmentStart = startOffset;
  let continuation = false;
  while (segmentStart < endOffset) {
    const segmentEnd = Math.min(endOffset, segmentStart + MAX_RENDER_SEGMENT_LENGTH);
    lines.push({
      content: content.slice(segmentStart, segmentEnd),
      lineNumber,
      continuation,
    });
    segmentStart = segmentEnd;
    continuation = true;
  }
}

function readTextPage(content: string, startOffset: number, firstLineNumber: number): TextPage {
  const lines: TextPageLine[] = [];
  let cursor = startOffset;
  let logicalLineCount = 0;

  while (logicalLineCount < PAGE_SIZE && cursor <= content.length) {
    const lineEnd = content.indexOf('\n', cursor);
    const endOffset = lineEnd === -1 ? content.length : lineEnd;
    appendLineSegments(lines, content, cursor, endOffset, firstLineNumber + logicalLineCount);
    logicalLineCount += 1;

    if (lineEnd === -1) {
      cursor = content.length + 1;
      break;
    }
    cursor = lineEnd + 1;
  }

  return {
    lines,
    logicalLineCount,
    nextStartOffset: cursor <= content.length ? cursor : undefined,
  };
}

function countLogicalLines(content: string) {
  if (!content) {
    return 1;
  }

  let count = 1;
  for (let index = 0; index < content.length; index += 1) {
    if (content.charCodeAt(index) === 10) {
      count += 1;
    }
  }
  return count;
}

export function TextViewer({
  content,
  encoding,
  searchQuery,
  isTruncated,
}: TextViewerProps) {
  const { t } = useTranslation();
  const [currentPage, setCurrentPage] = useState(0);
  const [pageStartOffsets, setPageStartOffsets] = useState([0]);
  const [localSearch, setLocalSearch] = useState(searchQuery || '');
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

    const observer = new ResizeObserver(() => {
      updateContainerHeight();
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    setCurrentPage(0);
    setPageStartOffsets([0]);
    setScrollTop(0);
    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
  }, [content]);

  useEffect(() => {
    setScrollTop(0);
    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
  }, [currentPage]);

  const pageStartLine = currentPage * PAGE_SIZE + 1;
  const currentPageData = useMemo(
    () => readTextPage(content, pageStartOffsets[currentPage] ?? 0, pageStartLine),
    [content, currentPage, pageStartLine, pageStartOffsets],
  );
  const totalLogicalLines = useMemo(() => countLogicalLines(content), [content]);
  const currentLines = currentPageData.lines;
  const hasNextPage = currentPageData.nextStartOffset !== undefined;
  const pageEndLine = pageStartLine + Math.max(0, currentPageData.logicalLineCount - 1);

  const handleScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(event.currentTarget.scrollTop);
  }, []);

  const visibleRange = useMemo(() => {
    const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_LINES);
    const visibleLineCount = Math.ceil(containerHeight / ROW_HEIGHT) + OVERSCAN_LINES * 2;
    const endIndex = Math.min(currentLines.length, startIndex + visibleLineCount);
    return { startIndex, endIndex };
  }, [containerHeight, currentLines.length, scrollTop]);

  const visibleLines = useMemo(
    () => currentLines.slice(visibleRange.startIndex, visibleRange.endIndex),
    [currentLines, visibleRange.endIndex, visibleRange.startIndex]
  );

  const isLargeContent =
    hasNextPage || currentPage > 0 || pageEndLine >= LARGE_TEXT_LINE_THRESHOLD;
  const visibleLineCount = visibleRange.endIndex - visibleRange.startIndex;

  // 高亮搜索关键词
  const highlightText = useCallback(
    (text: string) => {
      const query = searchQuery || localSearch;
      if (!query) return text;

      const escapedQuery = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const parts = text.split(new RegExp(`(${escapedQuery})`, 'gi'));

      return parts.map((part, i) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark key={i} className="bg-forensics-warning-bg px-0.5 rounded-none">
            {part}
          </mark>
        ) : (
          part
        )
      );
    },
    [searchQuery, localSearch]
  );

  // 计算当前页的行号宽度
  const lineNumberWidth = useMemo(() => {
    const maxLine = pageEndLine;
    const digits = String(maxLine).length;
    return Math.max(digits, 4) * 8 + 16; // 每位数字约 8px + padding
  }, [pageEndLine]);

  const goToNextPage = () => {
    const nextStartOffset = currentPageData.nextStartOffset;
    if (nextStartOffset === undefined) {
      return;
    }
    setPageStartOffsets((current) => {
      if (current[currentPage + 1] === nextStartOffset) {
        return current;
      }
      return [...current.slice(0, currentPage + 1), nextStartOffset];
    });
    setCurrentPage((page) => page + 1);
  };

  return (
    <div className="flex flex-col h-full">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b bg-forensics-panel text-[11px] shrink-0">
        <FileText size={12} className="text-forensics-muted" />
        <span className="text-forensics-muted font-light">{encoding}</span>
        <span className="text-forensics-border-strong">|</span>
        <span className="text-forensics-muted">{t('textViewer.lineCount', { count: totalLogicalLines })}</span>

        {isTruncated && (
          <>
            <span className="text-forensics-border-strong">|</span>
            <span className="text-forensics-warning-text">{t('textViewer.truncated')}</span>
          </>
        )}

        {isLargeContent && (
          <>
            <span className="text-forensics-border-strong">|</span>
            <span className="text-forensics-muted" role="status">
              {t('textViewer.largeContentMode', {
                start: pageStartLine.toLocaleString(),
                end: pageEndLine.toLocaleString(),
                rendered: visibleLineCount.toLocaleString(),
              })}
            </span>
          </>
        )}

        {/* 内联搜索 */}
        <div className="ml-auto flex items-center gap-1">
          <Search size={11} className="text-forensics-muted-light" />
          <Input
            type="text"
            value={localSearch}
            onChange={(e) => setLocalSearch(e.target.value)}
            placeholder={t('textViewer.searchPlaceholder')}
            variant="forensics"
            inputSize="inline"
            className="w-32 border-forensics-border-strong bg-forensics-surface placeholder:text-forensics-400 focus-visible:border-forensics-muted"
          />
        </div>

        {/* 分页控制 */}
        {(currentPage > 0 || hasNextPage) && (
          <div className="flex items-center gap-1 ml-2">
            <Button
              type="button"
              variant="viewerControl"
              size="iconXs"
              onClick={() => setCurrentPage((p) => Math.max(0, p - 1))}
              disabled={currentPage === 0}
              aria-label="上一页"
            >
              <ChevronLeft size={12} />
            </Button>
            <span className="text-forensics-muted w-16 text-center">
              {currentPage + 1}{hasNextPage ? '+' : ''}
            </span>
            <Button
              type="button"
              variant="viewerControl"
              size="iconXs"
              onClick={goToNextPage}
              disabled={!hasNextPage}
              aria-label="下一页"
            >
              <ChevronRight size={12} />
            </Button>
          </div>
        )}
      </div>

      {/* 文本内容 */}
      <div
        ref={containerRef}
        data-testid="text-scroll-container"
        className="min-h-0 flex-1 overflow-auto bg-forensics-surface"
        onScroll={handleScroll}
      >
        <div
          className="font-mono text-[11px] leading-[18px]"
          style={{ height: currentLines.length * ROW_HEIGHT, position: 'relative' }}
        >
          <div
            data-testid="text-visible-window"
            style={{
              position: 'absolute',
              top: visibleRange.startIndex * ROW_HEIGHT,
              left: 0,
              right: 0,
            }}
          >
            {visibleLines.map((line, index) => {
              const lineIndex = visibleRange.startIndex + index;
              const lineNum = line.lineNumber;
              const hasMatch =
                (searchQuery || localSearch) &&
                line.content
                  .toLowerCase()
                  .includes((searchQuery || localSearch).toLowerCase());

              return (
                <div
                  key={`${lineNum}:${lineIndex}`}
                  data-line-number={lineNum}
                  className={`flex hover:bg-forensics-highlight ${
                    hasMatch ? 'bg-forensics-warning-bg' : ''
                  }`}
                  style={{ height: ROW_HEIGHT }}
                >
                  {/* 行号 */}
                  <div
                    data-testid="text-line-number"
                    className="shrink-0 text-right text-forensics-muted-light select-none border-r border-forensics-border-light bg-forensics-panel px-2"
                    style={{ width: `${lineNumberWidth}px` }}
                  >
                    {line.continuation ? '...' : lineNum}
                  </div>
                  {/* 代码内容 */}
                  <div className="flex-1 px-3 whitespace-pre-wrap break-all min-w-0">
                    <span data-testid="text-line-content">
                      {highlightText(line.content) || '\u00A0'}
                    </span> {/* 空行也显示高度 */}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* 状态栏 */}
      <div className="flex items-center gap-3 px-3 py-1 border-t bg-forensics-panel text-[10px] text-forensics-muted-light shrink-0">
        <span>
          {localSearch
            ? t('textViewer.status.search', { query: localSearch })
            : t('textViewer.status.ready')}
        </span>
      </div>
    </div>
  );
}
