/**
 * DenseTableFilterBar - DenseDataTable 的搜索/筛选工具条（纯展示组件）。
 *
 * 状态由 useDenseTableFilter 持有；过滤只覆盖当前已加载行，
 * 激活时显示 "筛选 X / 已加载 Y" 及范围提示。
 */

import { useTranslation } from 'react-i18next';
import { Input } from '@/app/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
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
        <div
          key={select.key}
          className="flex items-center gap-1 text-forensics-text-tertiary"
        >
          <span className="shrink-0">{select.label}</span>
          <Select
            value={select.value || '__all__'}
            onValueChange={(value) => onSelectChange(select.key, value === '__all__' ? '' : value)}
          >
            <SelectTrigger aria-label={select.label} size="xs" variant="mono" className="max-w-[160px]">
              <SelectValue placeholder={t('denseTable.filterAll')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">{t('denseTable.filterAll')}</SelectItem>
            {select.options.map((option) => (
              <SelectItem key={option} value={option}>
                {option}
              </SelectItem>
            ))}
            </SelectContent>
          </Select>
        </div>
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
