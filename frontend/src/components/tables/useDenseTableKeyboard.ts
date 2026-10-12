import { useCallback, useEffect, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from '@/app/components/ui/toast';
import type { DenseColumn } from './dense-table-types';

export function useDenseTableKeyboard<T>({ columns, visibleRows, onRowClick, loadContextKey, scrollToIndex }: { columns: DenseColumn<T>[]; visibleRows: T[]; onRowClick?: (row: T) => void; loadContextKey?: string; scrollToIndex: (index: number, options: { align: 'auto' }) => void }) {
  const { t } = useTranslation();
  const [focusedIndex, setFocusedIndex] = useState(-1);
  useEffect(() => { setFocusedIndex(-1); }, [loadContextKey]);

  /** Focused-row copy: only columns that expose a text extractor contribute a cell. */
  const copyFocusedRow = useCallback((row: T) => {
    const line = columns.filter((column) => column.text).map((column) => column.text?.(row) ?? '').join('\t');
    const clipboard = typeof navigator === 'undefined' ? undefined : navigator.clipboard;
    if (!line || !clipboard?.writeText) return false;
    void clipboard.writeText(line);
    toast(t('denseTable.copied'));
    return true;
  }, [columns, t]);

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    const lastIndex = visibleRows.length - 1;
    if (lastIndex < 0) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      setFocusedIndex((current) => {
        if (event.key === 'Home') return 0;
        if (event.key === 'End') return lastIndex;
        const step = event.key === 'ArrowDown' ? 1 : -1;
        const base = current < 0 ? (step > 0 ? -1 : lastIndex + 1) : current;
        return Math.min(Math.max(base + step, 0), lastIndex);
      });
      return;
    }
    if (focusedIndex < 0) return;
    const row = visibleRows[focusedIndex];
    if (!row) return;
    if (event.key === 'Enter') {
      event.preventDefault();
      onRowClick?.(row);
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'c' && copyFocusedRow(row)) {
      event.preventDefault();
    }
  }, [copyFocusedRow, focusedIndex, onRowClick, visibleRows]);

  useEffect(() => {
    if (focusedIndex < 0) return;
    scrollToIndex(focusedIndex, { align: 'auto' });
  }, [focusedIndex, scrollToIndex]);

  return { focusedIndex, handleKeyDown };
}
