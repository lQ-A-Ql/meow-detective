/**
 * useDenseTableFilter - DenseDataTable 的客户端搜索/列筛选状态。
 *
 * 仅在 DenseDataTable 传入 `filterable` 时启用；关键词输入防抖 200ms，
 * 匹配范围是所有声明了 `text` 访问器的列（大小写不敏感包含匹配），
 * 列筛选对声明了 `filterable` 的列做单选等值过滤。过滤只作用于当前
 * 已加载行，不影响 onReachEnd 续载。
 */

import { useEffect, useMemo, useState } from 'react';
import type { DenseColumn } from './DenseDataTable';

const KEYWORD_DEBOUNCE_MS = 200;

type TextColumn<T> = DenseColumn<T> & { text: (row: T) => string };

function hasTextAccessor<T>(column: DenseColumn<T>): column is TextColumn<T> {
  return typeof column.text === 'function';
}

export interface DenseTableFilterSelect {
  key: string;
  label: string;
  value: string;
  options: string[];
}

interface UseDenseTableFilterArgs<T> {
  columns: DenseColumn<T>[];
  rows: T[];
  enabled: boolean;
  /** 数据上下文变化时清空筛选状态，避免把旧上下文的条件套到新数据上。 */
  resetKey?: string;
}

export function useDenseTableFilter<T>({
  columns,
  rows,
  enabled,
  resetKey,
}: UseDenseTableFilterArgs<T>) {
  const [keywordInput, setKeywordInput] = useState('');
  const [keyword, setKeyword] = useState('');
  const [columnFilters, setColumnFilters] = useState<Record<string, string>>({});

  useEffect(() => {
    const timer = window.setTimeout(() => setKeyword(keywordInput.trim()), KEYWORD_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [keywordInput]);

  useEffect(() => {
    setKeywordInput('');
    setKeyword('');
    setColumnFilters({});
  }, [resetKey]);

  const searchableColumns = useMemo(
    () => (enabled ? columns.filter(hasTextAccessor) : []),
    [columns, enabled],
  );
  const filterableColumns = useMemo(
    () => (enabled ? columns.filter((column) => column.filterable).filter(hasTextAccessor) : []),
    [columns, enabled],
  );

  const selects: DenseTableFilterSelect[] = useMemo(
    () =>
      filterableColumns.map((column) => {
        const seen = new Set<string>();
        for (const row of rows) {
          seen.add(column.text(row));
        }
        return {
          key: column.key,
          label: typeof column.title === 'string' ? column.title : column.key,
          value: columnFilters[column.key] ?? '',
          options: [...seen].sort((a, b) => a.localeCompare(b)),
        };
      }),
    [columnFilters, filterableColumns, rows],
  );

  const activeSelections = useMemo(
    () => Object.entries(columnFilters).filter(([, value]) => value !== ''),
    [columnFilters],
  );
  const normalizedKeyword = keyword.toLowerCase();
  const filterActive = normalizedKeyword.length > 0 || activeSelections.length > 0;

  const visibleRows = useMemo(() => {
    if (!enabled || !filterActive) return rows;
    return rows.filter((row) => {
      if (
        normalizedKeyword
        && !searchableColumns.some((column) =>
          column.text(row).toLowerCase().includes(normalizedKeyword))
      ) {
        return false;
      }
      return activeSelections.every(([key, value]) => {
        const column = filterableColumns.find((candidate) => candidate.key === key);
        return column ? column.text(row) === value : true;
      });
    });
  }, [
    activeSelections,
    enabled,
    filterActive,
    filterableColumns,
    normalizedKeyword,
    rows,
    searchableColumns,
  ]);

  const setSelectValue = (key: string, value: string) => {
    setColumnFilters((current) => ({ ...current, [key]: value }));
  };

  return {
    keywordInput,
    setKeywordInput,
    selects,
    setSelectValue,
    filterActive,
    visibleRows,
  };
}
