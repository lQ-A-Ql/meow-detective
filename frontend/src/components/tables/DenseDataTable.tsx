import { ReactNode } from 'react';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/app/components/ui/table';

export interface DenseColumn<T> {
  key: string;
  title: ReactNode;
  className?: string;
  render: (row: T) => ReactNode;
}

interface DenseDataTableProps<T> {
  columns: DenseColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  selectedRowKey?: string;
  onRowClick?: (row: T) => void;
  emptyTitle?: string;
  emptyDescription?: string;
}

export function DenseDataTable<T>({
  columns,
  rows,
  getRowKey,
  selectedRowKey,
  onRowClick,
  emptyTitle = '暂无记录',
  emptyDescription = '当前范围内没有可显示的数据。',
}: DenseDataTableProps<T>) {
  return (
    <div className="min-h-0 flex-1 overflow-auto bg-white font-mono text-[11px]">
      <Table className="text-[11px]">
        <TableHeader className="sticky top-0 z-10 bg-[#fafafa]">
          <TableRow className="border-b border-[#e0e0e0] hover:bg-[#fafafa]">
            {columns.map((column) => (
              <TableHead
                key={column.key}
                className={`h-7 border-r border-[#e0e0e0] px-2 text-[11px] font-medium tracking-wider text-[#555] last:border-r-0 ${column.className ?? ''}`}
              >
                {column.title}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.length === 0 ? (
            <TableRow className="hover:bg-white">
              <TableCell colSpan={columns.length} className="px-4 py-8">
                <div className="space-y-1 text-center font-sans">
                  <div className="text-[12px] font-medium text-[#222]">{emptyTitle}</div>
                  <div className="text-[11px] text-[#777]">{emptyDescription}</div>
                </div>
              </TableCell>
            </TableRow>
          ) : null}
          {rows.map((row) => {
            const key = getRowKey(row);
            const selected = key === selectedRowKey;
            return (
              <TableRow
                key={key}
                data-state={selected ? 'selected' : undefined}
                className={`cursor-pointer border-b ${selected ? 'bg-[#e8e8e8] text-[#111]' : 'text-[#333] hover:bg-[#f9f9f9]'}`}
                onClick={() => onRowClick?.(row)}
              >
                {columns.map((column, index) => (
                  <TableCell
                    key={column.key}
                    className={`px-2 py-1.5 align-middle ${index < columns.length - 1 ? 'border-r border-[#f0f0f0]' : ''} ${column.className ?? ''}`}
                  >
                    {column.render(row)}
                  </TableCell>
                ))}
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}
