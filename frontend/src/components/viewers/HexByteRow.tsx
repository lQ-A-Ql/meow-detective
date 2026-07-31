import { memo } from 'react';
import { cn } from '@/app/components/ui/utils';
import {
  formatAsciiByte,
  formatByte,
  formatOffset,
  type HexParsedLine,
} from '@/components/viewers/hex-viewer-model';

interface HexByteRowProps {
  line: HexParsedLine;
  lineIndex: number;
  lineHeight: number;
  offsetWidth: number;
  highlightedOffset?: number;
  selectionStart?: number;
  selectionEnd?: number;
  selectionFocus?: number;
}

export const HexByteRow = memo(function HexByteRow({
  line,
  lineIndex,
  lineHeight,
  offsetWidth,
  highlightedOffset,
  selectionStart,
  selectionEnd,
  selectionFocus,
}: HexByteRowProps) {
  const cells = line.bytes.map((byte, byteIndex) => ({
    absoluteOffset: line.offsetValue + byteIndex,
    ascii: formatAsciiByte(byte),
    byteIndex,
    hex: formatByte(byte),
  }));

  const isSelected = (absoluteOffset: number) => (
    selectionStart !== undefined
    && selectionEnd !== undefined
    && absoluteOffset >= selectionStart
    && absoluteOffset <= selectionEnd
  );

  const byteCellClass = (absoluteOffset: number, widthClass: string) => {
    const selected = isSelected(absoluteOffset);
    return cn(
      'inline-flex shrink-0 cursor-crosshair select-none items-center justify-center font-mono text-[11px] leading-none tracking-normal transition-none',
      widthClass,
      highlightedOffset === absoluteOffset || selected
        ? 'bg-[var(--forensics-selection-bg)] text-[var(--forensics-selection-text)]'
        : '',
      selectionFocus === absoluteOffset
        ? 'ring-1 ring-inset ring-forensics-sakura-500'
        : '',
    );
  };

  return (
    <div
      data-row-index={lineIndex}
      role="row"
      className="flex hover:bg-forensics-panel-strong"
      style={{ height: lineHeight }}
    >
      <div
        role="rowheader"
        className="shrink-0 border-r border-forensics-border-light bg-forensics-panel pr-2 text-right text-forensics-muted-lighter select-none"
        style={{ width: offsetWidth }}
      >
        {line.offset}
      </div>
      <div
        role="presentation"
        className="flex min-w-[52ch] flex-1 items-center gap-[1ch] whitespace-pre px-3 text-forensics-text-secondary"
      >
        {cells.length > 0
          ? cells.map((cell) => (
            <span
              key={cell.absoluteOffset}
              role="gridcell"
              aria-label={`offset 0x${formatOffset(cell.absoluteOffset)}，Hex ${cell.hex}`}
              aria-selected={isSelected(cell.absoluteOffset)}
              className={cn(
                byteCellClass(cell.absoluteOffset, 'w-[2ch]'),
                cell.byteIndex === 7 && cells.length > 8 ? 'mr-[1ch]' : '',
              )}
              data-byte-offset={cell.absoluteOffset}
              data-byte-side="hex"
              data-highlighted={
                highlightedOffset === cell.absoluteOffset || isSelected(cell.absoluteOffset) || undefined
              }
              data-selected={isSelected(cell.absoluteOffset) || undefined}
              data-testid={`hex-byte-${cell.absoluteOffset}`}
            >
              {cell.hex}
            </span>
          ))
          : line.fallbackHex}
      </div>
      <div
        role="presentation"
        className="flex w-[18ch] shrink-0 items-center whitespace-pre border-l border-forensics-border-light pl-2 text-forensics-muted"
      >
        {cells.map((cell) => (
          <span
            key={cell.absoluteOffset}
            role="gridcell"
            aria-label={`offset 0x${formatOffset(cell.absoluteOffset)}，ASCII ${cell.ascii}`}
            aria-selected={isSelected(cell.absoluteOffset)}
            className={byteCellClass(cell.absoluteOffset, 'w-[1ch]')}
            data-byte-offset={cell.absoluteOffset}
            data-byte-side="ascii"
            data-highlighted={
              highlightedOffset === cell.absoluteOffset || isSelected(cell.absoluteOffset) || undefined
            }
            data-selected={isSelected(cell.absoluteOffset) || undefined}
            data-testid={`ascii-byte-${cell.absoluteOffset}`}
          >
            {cell.ascii}
          </span>
        ))}
      </div>
    </div>
  );
});
