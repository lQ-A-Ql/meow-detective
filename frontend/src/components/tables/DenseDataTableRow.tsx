import { memo, type ReactElement } from 'react';
import { TableCell, TableRow } from '@/app/components/ui/table';
import { HorizontalScroll } from '@/components/layout/HorizontalScroll';
import type { DenseColumn } from './DenseDataTable';

export interface DenseDataTableRowProps<T> {
  row: T;
  columns: DenseColumn<T>[];
  selected: boolean;
  onRowClick?: (row: T) => void;
  onRowDoubleClick?: (row: T) => void;
  renderRowContextMenu?: (row: T, trigger: ReactElement) => ReactElement;
}

function DenseDataTableRowBase<T>({
  row,
  columns,
  selected,
  onRowClick,
  onRowDoubleClick,
  renderRowContextMenu,
}: DenseDataTableRowProps<T>) {
  const handleRowClick = () => {
    if (typeof window !== 'undefined') {
      const selection = window.getSelection();
      if (selection && !selection.isCollapsed) return;
    }
    onRowClick?.(row);
  };

  const tableRow = (
    <TableRow
      data-state={selected ? 'selected' : undefined}
      className={`h-[31px] cursor-pointer border-b ${
        selected
          ? 'bg-forensics-sakura-150 text-forensics-text'
          : 'text-forensics-text-secondary hover:bg-forensics-hover'
      }`}
      onClick={handleRowClick}
      onDoubleClick={() => onRowDoubleClick?.(row)}
    >
      {columns.map((column, index) => (
        <TableCell
          key={column.key}
          className={`h-[31px] px-2 py-1.5 align-middle ${
            index < columns.length - 1
              ? 'border-r border-forensics-border-light'
              : ''
          } ${column.className ?? ''}`}
        >
          <HorizontalScroll
            variant="cell"
            revealOnHover
            className="whitespace-nowrap text-inherit"
          >
            <div className="min-w-full w-max select-text pr-2">
              {column.render(row)}
            </div>
          </HorizontalScroll>
        </TableCell>
      ))}
    </TableRow>
  );
  return renderRowContextMenu?.(row, tableRow) ?? tableRow;
}

export const DenseDataTableRow = memo(DenseDataTableRowBase) as <T>(
  props: DenseDataTableRowProps<T>,
) => ReactElement;
