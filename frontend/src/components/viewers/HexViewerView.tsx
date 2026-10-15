import type { useHexViewerModel } from '../../hooks/use-hex-viewer-model';
import { HexByteRow } from '@/components/viewers/HexByteRow';

export function HexViewerView({ model }: { model: ReturnType<typeof useHexViewerModel> }) {
const { lineHeight, loadedRanges, t, containerRef, handlePointerLeave, hoveredOffset, selection, rowCount, visibleRange, visibleLines, handleScroll, handlePointerMove, handlePointerDown, finishPointerSelection, handleKeyDown, offsetWidth, selectionStart, selectionEnd } = model;



  return (
    <div className="flex h-full min-h-0 flex-col bg-forensics-surface font-mono text-[12px]">
      <div
        ref={containerRef}
        data-testid="hex-scroll-container"
        className="forensics-scrollbar min-h-0 flex-1 touch-pan-y select-none overflow-auto"
        role="grid"
        tabIndex={rowCount > 0 ? 0 : -1}
        aria-label={t('viewer.hex.ariaLabel')}
        onKeyDown={handleKeyDown}
        onPointerCancel={finishPointerSelection}
        onPointerDown={handlePointerDown}
        onPointerLeave={handlePointerLeave}
        onPointerMove={handlePointerMove}
        onPointerUp={finishPointerSelection}
        onScroll={handleScroll}
      >
        <div style={{ height: rowCount * lineHeight, position: 'relative' }}>
          <div
            data-testid="hex-visible-window"
            style={{
              position: 'absolute',
              top: visibleRange.startIndex * lineHeight,
              width: '100%',
            }}
          >
            {visibleLines.map(({ parsed: line, lineIndex }) => (
              <HexByteRow
                key={lineIndex}
                line={line}
                lineIndex={lineIndex}
                lineHeight={lineHeight}
                offsetWidth={offsetWidth}
                highlightedOffset={hoveredOffset}
                selectionStart={selectionStart}
                selectionEnd={selectionEnd}
                selectionFocus={selection?.focus}
              />
            ))}
          </div>
        </div>

        {rowCount === 0 && (
          <div className="flex h-full items-center justify-center text-forensics-muted-lighter">
            {t('viewer.hex.empty')}
          </div>
        )}
      </div>

      {loadedRanges?.length ? (
        <div className="shrink-0 border-t border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted">
          {t('viewer.hex.loadedRanges')}: {loadedRanges.map((range) => `0x${range.start.toString(16).toUpperCase()}-0x${Math.max(range.start, range.end - 1).toString(16).toUpperCase()}`).join(', ')}
        </div>
      ) : null}
    </div>
  );
}
