import type { ReactNode } from 'react';
import { cn } from '@/app/components/ui/utils';
import {
  DENSE_TABLE_HEADER_HEIGHT,
  DENSE_TABLE_ROW_HEIGHT,
} from './dense-table-metrics';

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

/** Provides the definite viewport required by DenseDataTable virtualization. */
export function DenseDataTableFrame({
  children,
  className,
  header,
  layout = 'embedded',
  maxHeight = 'standard',
  rowCount,
  variant = 'bordered',
}: DenseDataTableFrameProps) {
  const contentHeight = DENSE_TABLE_HEADER_HEIGHT
    + (header ? FRAME_HEADER_HEIGHT : 0)
    + Math.max(0, rowCount ?? 0) * DENSE_TABLE_ROW_HEIGHT
    + (variant === 'bordered' ? FRAME_BORDER_HEIGHT : 0);
  const embeddedHeight = rowCount === 0 ? EMPTY_HEIGHT : contentHeight;
  const embeddedStyle = layout === 'embedded'
    ? { height: `min(${embeddedHeight}px, min(60vh, ${MAX_HEIGHT[maxHeight]}))` }
    : undefined;

  return (
    <div
      className={cn(
        'flex min-h-0 flex-col overflow-hidden bg-forensics-surface',
        layout === 'fill' && 'flex-1',
        variant === 'bordered' && 'border border-forensics-border',
        className,
      )}
      style={embeddedStyle}
    >
      {header}
      {children}
    </div>
  );
}
