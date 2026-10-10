import type { ReactNode } from 'react';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/app/components/ui/table';

export interface SummaryTableColumn {
  key: string;
  label: ReactNode;
  /** Class applied to both the header cell and every cell in this column. */
  className?: string;
}

/** A cell is either a node or a node plus the `title` tooltip the cell should carry. */
export type SummaryTableCell = ReactNode | { content: ReactNode; title?: string };

export interface SummaryTableRow {
  key: string | number;
  cells: SummaryTableCell[];
  selected?: boolean;
  onSelect?: () => void;
  className?: string;
}

function isCellObject(cell: SummaryTableCell): cell is { content: ReactNode; title?: string } {
  return typeof cell === 'object' && cell !== null && !Array.isArray(cell) && 'content' in cell;
}

/**
 * Centralised primitive for read-only semantic summary tables (header + rows, no
 * virtualisation). Feature components compose this instead of importing
 * `@/app/components/ui/table` themselves, so the low-level table primitives stay
 * owned by the shared table layer.
 */
export function SummaryTable({
  columns,
  rows,
  minWidthClass = 'min-w-[720px]',
  className = 'text-left text-xs',
}: {
  columns: SummaryTableColumn[];
  rows: SummaryTableRow[];
  minWidthClass?: string;
  className?: string;
}) {
  return (
    <Table className={`${minWidthClass} ${className}`}>
      <TableHeader>
        <TableRow>
          {columns.map((column) => (
            <TableHead key={column.key} className={column.className}>
              {column.label}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((row) => (
          <TableRow
            key={row.key}
            data-state={row.selected ? 'selected' : undefined}
            onClick={row.onSelect}
            className={row.className}
          >
            {row.cells.map((cell, index) => {
              const column = columns[index];
              const title = isCellObject(cell) ? cell.title : undefined;
              return (
                <TableCell key={column?.key ?? index} className={column?.className} title={title}>
                  {isCellObject(cell) ? cell.content : cell}
                </TableCell>
              );
            })}
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
