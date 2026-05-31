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

import { useState, useMemo, useRef, useCallback } from 'react';
import { ChevronLeft, ChevronRight, FileText, Search } from 'lucide-react';

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

export function TextViewer({
  content,
  encoding,
  searchQuery,
  isTruncated,
}: TextViewerProps) {
  const [currentPage, setCurrentPage] = useState(0);
  const [localSearch, setLocalSearch] = useState(searchQuery || '');
  const pageSize = 1000; // 每页行数
  const containerRef = useRef<HTMLDivElement>(null);

  // 分割为行
  const lines = useMemo(() => content.split('\n'), [content]);

  // 分页
  const totalPages = Math.ceil(lines.length / pageSize);
  const currentLines = useMemo(
    () => lines.slice(currentPage * pageSize, (currentPage + 1) * pageSize),
    [lines, currentPage, pageSize]
  );

  // 高亮搜索关键词
  const highlightText = useCallback(
    (text: string) => {
      const query = searchQuery || localSearch;
      if (!query) return text;

      const escapedQuery = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const parts = text.split(new RegExp(`(${escapedQuery})`, 'gi'));

      return parts.map((part, i) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark key={i} className="bg-yellow-200 px-0.5 rounded">
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
    const maxLine = Math.min((currentPage + 1) * pageSize, lines.length);
    const digits = String(maxLine).length;
    return Math.max(digits, 4) * 8 + 16; // 每位数字约 8px + padding
  }, [currentPage, lines.length, pageSize]);

  return (
    <div className="flex flex-col h-full">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b bg-[#fafafa] text-[11px] shrink-0">
        <FileText size={12} className="text-[#666]" />
        <span className="text-[#666] font-medium">{encoding}</span>
        <span className="text-[#ddd]">|</span>
        <span className="text-[#666]">{lines.length.toLocaleString()} 行</span>

        {isTruncated && (
          <>
            <span className="text-[#ddd]">|</span>
            <span className="text-amber-600">已截断</span>
          </>
        )}

        {/* 内联搜索 */}
        <div className="ml-auto flex items-center gap-1">
          <Search size={11} className="text-[#999]" />
          <input
            type="text"
            value={localSearch}
            onChange={(e) => setLocalSearch(e.target.value)}
            placeholder="搜索..."
            className="w-32 px-1.5 py-0.5 text-[11px] border border-[#ddd] rounded bg-white 
                       focus:outline-none focus:border-[#999] placeholder:text-[#ccc]"
          />
        </div>

        {/* 分页控制 */}
        {totalPages > 1 && (
          <div className="flex items-center gap-1 ml-2">
            <button
              onClick={() => setCurrentPage((p) => Math.max(0, p - 1))}
              disabled={currentPage === 0}
              className="p-0.5 hover:bg-[#e0e0e0] rounded disabled:opacity-30"
            >
              <ChevronLeft size={12} />
            </button>
            <span className="text-[#666] w-16 text-center">
              {currentPage + 1}/{totalPages}
            </span>
            <button
              onClick={() =>
                setCurrentPage((p) => Math.min(totalPages - 1, p + 1))
              }
              disabled={currentPage === totalPages - 1}
              className="p-0.5 hover:bg-[#e0e0e0] rounded disabled:opacity-30"
            >
              <ChevronRight size={12} />
            </button>
          </div>
        )}
      </div>

      {/* 文本内容 */}
      <div
        ref={containerRef}
        className="flex-1 overflow-auto bg-white"
      >
        <div className="font-mono text-[11px] leading-[18px]">
          {currentLines.map((line, index) => {
            const lineNum = currentPage * pageSize + index + 1;
            const hasMatch =
              (searchQuery || localSearch) &&
              line
                .toLowerCase()
                .includes((searchQuery || localSearch).toLowerCase());

            return (
              <div
                key={lineNum}
                className={`flex hover:bg-[#f8f8f8] ${
                  hasMatch ? 'bg-yellow-50' : ''
                }`}
              >
                {/* 行号 */}
                <div
                  className="shrink-0 text-right text-[#999] select-none border-r border-[#eee] bg-[#fafafa] px-2"
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

      {/* 状态栏 */}
      <div className="flex items-center gap-3 px-3 py-1 border-t bg-[#fafafa] text-[10px] text-[#999] shrink-0">
        <span>
          {localSearch
            ? `搜索: "${localSearch}"`
            : '就绪'}
        </span>
      </div>
    </div>
  );
}
