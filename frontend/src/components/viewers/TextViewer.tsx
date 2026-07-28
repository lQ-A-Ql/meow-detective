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
import { ScrollArea } from '@/app/components/ui/scroll-area';

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

export function TextViewer({
  content,
  encoding,
  searchQuery,
  isTruncated,
}: TextViewerProps) {
  const { t } = useTranslation();
  const [currentPage, setCurrentPage] = useState(0);
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

  // 分割为行
  const lines = useMemo(() => content.split('\n'), [content]);

  // 分页
  const totalPages = Math.ceil(lines.length / PAGE_SIZE);
  const currentLines = useMemo(
    () => lines.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE),
    [lines, currentPage]
  );

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

  const isLargeContent = lines.length >= LARGE_TEXT_LINE_THRESHOLD;
  const visibleLineCount = visibleRange.endIndex - visibleRange.startIndex;
  const pageStartLine = currentPage * PAGE_SIZE + 1;
  const pageEndLine = Math.min((currentPage + 1) * PAGE_SIZE, lines.length);

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
    const maxLine = Math.min((currentPage + 1) * PAGE_SIZE, lines.length);
    const digits = String(maxLine).length;
    return Math.max(digits, 4) * 8 + 16; // 每位数字约 8px + padding
  }, [currentPage, lines.length]);

  return (
    <div className="flex flex-col h-full">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b bg-forensics-panel text-[11px] shrink-0">
        <FileText size={12} className="text-forensics-muted" />
        <span className="text-forensics-muted font-light">{encoding}</span>
        <span className="text-forensics-border-strong">|</span>
        <span className="text-forensics-muted">{t('textViewer.lineCount', { count: lines.length })}</span>

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
        {totalPages > 1 && (
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
              {currentPage + 1}/{totalPages}
            </span>
            <Button
              type="button"
              variant="viewerControl"
              size="iconXs"
              onClick={() =>
                setCurrentPage((p) => Math.min(totalPages - 1, p + 1))
              }
              disabled={currentPage === totalPages - 1}
              aria-label="下一页"
            >
              <ChevronRight size={12} />
            </Button>
          </div>
        )}
      </div>

      {/* 文本内容 */}
      <ScrollArea
        className="min-h-0 flex-1 bg-forensics-surface"
        viewportRef={containerRef}
        viewportTestId="text-scroll-container"
        viewportProps={{
          onScroll: handleScroll,
        }}
        showHorizontalScrollbar
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
              const lineNum = currentPage * PAGE_SIZE + lineIndex + 1;
              const hasMatch =
                (searchQuery || localSearch) &&
                line
                  .toLowerCase()
                  .includes((searchQuery || localSearch).toLowerCase());

              return (
                <div
                  key={lineNum}
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
                    {lineNum}
                  </div>
                  {/* 代码内容 */}
                  <div className="flex-1 px-3 whitespace-pre-wrap break-all min-w-0">
                    {highlightText(line) || '\u00A0'} {/* 空行也显示高度 */}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </ScrollArea>

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
