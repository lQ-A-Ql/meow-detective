import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';
import { DENSE_TABLE_HEADER_HEIGHT, DENSE_TABLE_ROW_HEIGHT } from './dense-table-metrics';
import { useDenseTableFrameLayout } from './useDenseTableFrameLayout';

type DenseDataTableFrameProps =
  | {
      children: ReactNode;
      className?: string;
      header?: ReactNode;
      layout: 'fill';
      maxHeight?: never;
      rowCount?: never;
      variant?: 'bordered' | 'plain';
    }
  | {
      children: ReactNode;
      className?: string;
      header?: ReactNode;
      layout?: 'embedded';
      maxHeight?: 'compact' | 'standard';
      rowCount: number;
      variant?: 'bordered' | 'plain';
    };

const EMPTY_HEIGHT = 128;
const FRAME_HEADER_HEIGHT = 34;
const FRAME_BORDER_HEIGHT = 2;

const MAX_HEIGHT = {
  compact: '35rem',
  standard: '45rem',
} as const;

/** Bounds the table viewport using its rendered content and the selected height limit. */
export function DenseDataTableFrame({
  children,
  className,
  header,
  layout = 'embedded',
  maxHeight = 'standard',
  rowCount,
  variant = 'bordered',
}: DenseDataTableFrameProps) {
  const estimatedContentHeight =
    DENSE_TABLE_HEADER_HEIGHT +
    (header ? FRAME_HEADER_HEIGHT : 0) +
    Math.max(0, rowCount ?? 0) * DENSE_TABLE_ROW_HEIGHT +
    (variant === 'bordered' ? FRAME_BORDER_HEIGHT : 0);
  const estimatedHeight = rowCount === 0 ? EMPTY_HEIGHT : estimatedContentHeight;
  const maximumHeight = `min(60vh, ${MAX_HEIGHT[maxHeight]})`;
  const initialHeight = layout === 'embedded' ? `min(${estimatedHeight}px, ${maximumHeight})` : undefined;
  const frameRef = useDenseTableFrameLayout(initialHeight, maximumHeight);

  return (
    <div
      ref={frameRef}
      data-slot="dense-table-frame"
      className={cn(
        'flex min-h-0 flex-col overflow-hidden bg-forensics-surface',
        layout === 'fill' && 'flex-1',
        layout === 'embedded' && 'shrink-0',
        variant === 'bordered' && 'border border-forensics-border',
        className,
      )}
      style={initialHeight ? { height: initialHeight } : undefined}
    >
      {header ? (
        <div data-slot="dense-table-frame-header" className="shrink-0">
          {header}
        </div>
      ) : null}
      {children}
    </div>
  );
}
