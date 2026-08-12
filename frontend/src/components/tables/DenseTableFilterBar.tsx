/**
 * DenseTableFilterBar - DenseDataTable 的搜索/筛选工具条（纯展示组件）。
 *
 * 状态由 useDenseTableFilter 持有；过滤只覆盖当前已加载行，
 * 激活时显示 "筛选 X / 已加载 Y" 及范围提示。
 */

import { useTranslation } from 'react-i18next';
import { Input } from '@/app/components/ui/input';
import type { DenseTableFilterSelect } from './useDenseTableFilter';

interface DenseTableFilterBarProps {
  keyword: string;
  onKeywordChange: (value: string) => void;
  selects: DenseTableFilterSelect[];
  onSelectChange: (key: string, value: string) => void;
  filterActive: boolean;
  filteredCount: number;
  loadedCount: number;
}

export function DenseTableFilterBar({
  keyword,
  onKeywordChange,
  selects,
  onSelectChange,
  filterActive,
  filteredCount,
  loadedCount,
}: DenseTableFilterBarProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-forensics-border bg-forensics-panel px-2 py-1.5 text-[11px]">
      <Input
        type="search"
        variant="mono"
        inputSize="inline"
        value={keyword}
        onChange={(event) => onKeywordChange(event.target.value)}
        placeholder={t('denseTable.searchPlaceholder')}
        aria-label={t('denseTable.searchPlaceholder')}
        className="min-w-[180px] flex-1 bg-forensics-surface"
      />
      {selects.map((select) => (
        <label
          key={select.key}
          className="flex items-center gap-1 text-forensics-text-tertiary"
        >
          <span className="shrink-0">{select.label}</span>
          <select
            value={select.value}
            onChange={(event) => onSelectChange(select.key, event.target.value)}
            aria-label={select.label}
            className="h-6 max-w-[160px] rounded-none border border-forensics-border-strong bg-forensics-surface px-1 font-mono text-[11px] text-forensics-text focus:outline-none focus-visible:border-forensics-sakura-500"
          >
            <option value="">{t('denseTable.filterAll')}</option>
            {select.options.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>
      ))}
      {filterActive ? (
        <span className="font-mono text-forensics-muted">
          {t('denseTable.filterSummary', { filtered: filteredCount, loaded: loadedCount })}
          <span className="ml-1 text-forensics-muted-light">
            {t('denseTable.filterScopeNote')}
          </span>
        </span>
      ) : null}
    </div>
  );
}
