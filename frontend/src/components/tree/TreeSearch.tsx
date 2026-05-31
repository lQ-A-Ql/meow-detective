/**
 * TreeSearch - 树搜索过滤组件
 *
 * 提供实时搜索过滤功能，支持：
 * - 模糊匹配
 * - 大小写不敏感
 * - 清除按钮
 */

import { useState, useCallback, useEffect } from 'react';
import { Search, X } from 'lucide-react';

interface TreeSearchProps {
  /** 过滤回调 */
  onFilter: (query: string) => void;
  /** 占位符文本 */
  placeholder?: string;
  /** 防抖延迟 (ms) */
  debounceMs?: number;
}

export function TreeSearch({
  onFilter,
  placeholder = '过滤目录...',
  debounceMs = 150,
}: TreeSearchProps) {
  const [query, setQuery] = useState('');

  // 防抖处理
  useEffect(() => {
    const timer = setTimeout(() => {
      onFilter(query);
    }, debounceMs);

    return () => clearTimeout(timer);
  }, [query, onFilter, debounceMs]);

  const handleClear = useCallback(() => {
    setQuery('');
    onFilter('');
  }, [onFilter]);

  return (
    <div className="px-2 py-1.5 border-b border-[#e0e0e0] bg-[#fafafa]">
      <div className="relative">
        <Search
          size={12}
          className="absolute left-2 top-1/2 -translate-y-1/2 text-[#999]"
        />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={placeholder}
          className="w-full pl-7 pr-6 py-1 text-[11px] border border-[#ddd] rounded bg-white 
                     focus:outline-none focus:border-[#999] placeholder:text-[#bbb] font-mono"
        />
        {query && (
          <button
            onClick={handleClear}
            className="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 hover:bg-[#f0f0f0] rounded"
            title="清除"
          >
            <X size={10} className="text-[#999]" />
          </button>
        )}
      </div>
    </div>
  );
}
