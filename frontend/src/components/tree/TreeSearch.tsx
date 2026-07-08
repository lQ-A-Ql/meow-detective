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
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';

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
        <Input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={placeholder}
          variant="mono"
          inputSize="compact"
          className="pl-7 pr-6"
        />
        {query && (
          <Button
            type="button"
            variant="viewerControl"
            size="iconXs"
            onClick={handleClear}
            className="absolute right-1.5 top-1/2 -translate-y-1/2 text-[#999]"
            title="清除"
            aria-label="清除"
          >
            <X size={10} className="text-[#999]" />
          </Button>
        )}
      </div>
    </div>
  );
}
