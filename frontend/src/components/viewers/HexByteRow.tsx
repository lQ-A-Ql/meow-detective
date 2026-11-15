import { memo } from 'react';
import { cn } from '@/app/components/ui/utils';
import { formatAsciiByte, formatByte, formatOffset, type HexParsedLine } from '@/components/viewers/hex-viewer-model';
import { asciiBandGeometry, hexBandGeometry, type HexBandGeometry } from '@/components/viewers/hex-band-layout';
import { searchSpansForRow } from '@/components/viewers/hex-search-spans';

interface HexByteRowProps {
  line: HexParsedLine;
  lineIndex: number;
  lineHeight: number;
  offsetWidth: number;
  highlightedOffset?: number;
  selectionStart?: number;
  selectionEnd?: number;
  selectionFocus?: number;
  searchRanges?: ReadonlyArray<{ start: number; end: number }>;
  activeMatchStart?: number;
}

interface BandProps {
  geometry: HexBandGeometry;
  className: string;
  testId: string;
}

function Band({ geometry, className, testId }: BandProps) {
  if (geometry.widthCh <= 0) return null;
  return (
    <span
      aria-hidden="true"
      data-testid={testId}
      className={cn('pointer-events-none absolute inset-y-[2px] rounded-[2px]', className)}
      style={{ left: `${geometry.leftCh}ch`, width: `${geometry.widthCh}ch` }}
    />
  );
}

function clippedBand(absoluteStart: number, absoluteEnd: number, rowStart: number, rowEnd: number) {
  const start = Math.max(absoluteStart, rowStart);
  const end = Math.min(absoluteEnd, rowEnd);
  return end > start ? { start: start - rowStart, count: end - start } : undefined;
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
  searchRanges,
  activeMatchStart,
}: HexByteRowProps) {
  const cells = line.bytes.map((byte, byteIndex) => ({
    absoluteOffset: line.offsetValue + byteIndex,
    ascii: formatAsciiByte(byte),
    byteIndex,
    hex: formatByte(byte),
  }));
  const rowStart = line.offsetValue;
  const rowEnd = rowStart + cells.length;
  const searchSpans = searchRanges ? searchSpansForRow(searchRanges, rowStart, rowEnd) : [];

  const isSelected = (absoluteOffset: number) =>
    selectionStart !== undefined &&
    selectionEnd !== undefined &&
    absoluteOffset >= selectionStart &&
    absoluteOffset <= selectionEnd;

  const selectionBand =
    selectionStart !== undefined && selectionEnd !== undefined
      ? clippedBand(selectionStart, selectionEnd + 1, rowStart, rowEnd)
      : undefined;
  const activeRange =
    activeMatchStart !== undefined ? searchRanges?.find((range) => range.start === activeMatchStart) : undefined;
  const activeBand = activeRange ? clippedBand(activeRange.start, activeRange.end, rowStart, rowEnd) : undefined;
  const isActive = (localIndex: number) =>
    activeBand !== undefined && localIndex >= activeBand.start && localIndex < activeBand.start + activeBand.count;

  const byteCellClass = (absoluteOffset: number, localIndex: number, widthClass: string) => {
    const selected = isSelected(absoluteOffset);
    return cn(
      'relative z-10 inline-flex shrink-0 cursor-crosshair select-none items-center justify-center font-mono text-[12px] leading-none tracking-normal transition-none',
      widthClass,
      highlightedOffset === absoluteOffset
        ? 'bg-[var(--forensics-hover)] text-[var(--forensics-text)]'
        : isActive(localIndex)
          ? 'text-[var(--forensics-hex-match-active-text)]'
          : '',
      selected && !isActive(localIndex) ? 'text-[var(--forensics-hex-selection-text)]' : '',
      selectionFocus === absoluteOffset ? 'ring-1 ring-inset ring-forensics-sakura-500' : '',
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
        className="flex min-w-[52ch] flex-1 items-center whitespace-pre px-3 text-forensics-text-secondary"
      >
        <div className="relative flex min-w-[52ch] flex-1 items-center gap-[1ch]">
          {searchSpans.map((span) => (
            <Band
              key={`hex-search-${span.start}`}
              geometry={hexBandGeometry(span.start, span.end - span.start)}
              className="z-0 bg-[var(--forensics-hex-match-bg)]"
              testId="hex-search-band"
            />
          ))}
          {selectionBand ? (
            <Band
              geometry={hexBandGeometry(selectionBand.start, selectionBand.count)}
              className="z-[1] bg-[var(--forensics-hex-selection-bg)]"
              testId="hex-selection-band"
            />
          ) : null}
          {activeBand ? (
            <Band
              geometry={hexBandGeometry(activeBand.start, activeBand.count)}
              className="z-[2] bg-[var(--forensics-hex-match-active-bg)]"
              testId="hex-active-match-band"
            />
          ) : null}
          {cells.length > 0
            ? cells.map((cell) => (
                <span
                  key={cell.absoluteOffset}
                  role="gridcell"
                  aria-label={`offset 0x${formatOffset(cell.absoluteOffset)}，Hex ${cell.hex}`}
                  aria-selected={isSelected(cell.absoluteOffset)}
                  className={cn(
                    byteCellClass(cell.absoluteOffset, cell.byteIndex, 'w-[2ch]'),
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
      </div>
      <div
        role="presentation"
        className="flex w-[20ch] shrink-0 items-center whitespace-pre border-l border-forensics-border-light pl-2 pr-1 text-forensics-muted"
      >
        <div className="relative flex items-center">
          {searchSpans.map((span) => (
            <Band
              key={`ascii-search-${span.start}`}
              geometry={asciiBandGeometry(span.start, span.end - span.start)}
              className="z-0 bg-[var(--forensics-hex-match-bg)]"
              testId="ascii-search-band"
            />
          ))}
          {selectionBand ? (
            <Band
              geometry={asciiBandGeometry(selectionBand.start, selectionBand.count)}
              className="z-[1] bg-[var(--forensics-hex-selection-bg)]"
              testId="ascii-selection-band"
            />
          ) : null}
          {activeBand ? (
            <Band
              geometry={asciiBandGeometry(activeBand.start, activeBand.count)}
              className="z-[2] bg-[var(--forensics-hex-match-active-bg)]"
              testId="ascii-active-match-band"
            />
          ) : null}
          {cells.map((cell) => (
            <span
              key={cell.absoluteOffset}
              role="gridcell"
              aria-label={`offset 0x${formatOffset(cell.absoluteOffset)}，ASCII ${cell.ascii}`}
              aria-selected={isSelected(cell.absoluteOffset)}
              className={cn(
                byteCellClass(cell.absoluteOffset, cell.byteIndex, 'w-[1ch]'),
                cell.byteIndex === 7 && cells.length > 8 ? 'mr-[1ch]' : '',
              )}
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
    </div>
  );
});
