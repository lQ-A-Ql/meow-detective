/**
 * TreeSearch - 树搜索过滤组件
 *
 * 提供实时搜索过滤功能，支持：
 * - 模糊匹配
 * - 大小写不敏感
 * - 清除按钮
 */

import { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Search } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
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
  placeholder,
  debounceMs = 150,
}: TreeSearchProps) {
  const { t } = useTranslation();
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
    <div className="border-b border-forensics-divider px-2 py-1.5">
      <div className="relative">
        <Search
          size={12}
          className="absolute left-2 top-1/2 -translate-y-1/2 text-forensics-muted-lighter"
        />
        <Input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={placeholder ?? t('tree.filterPlaceholder')}
          variant="search"
          inputSize="compact"
          className="h-6 py-0 pl-7 pr-6 text-[12px] leading-6"
        />
        {query ? (
          <Tooltip content={t('tree.clear')}>
            <Button
              type="button"
              variant="forensicsGhost"
              onClick={handleClear}
              className="absolute right-1 top-1/2 -translate-y-1/2 text-forensics-ink-3"
              aria-label={t('tree.clear')}
            >
              <ActionIcon name="close" className="size-[11px]" />
            </Button>
          </Tooltip>
        ) : null}
      </div>
    </div>
  );
}
