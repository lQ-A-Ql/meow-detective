import { useTranslation } from 'react-i18next';
import { SearchHighlight } from '@/app/components/ui/search-highlight';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { TEXT_ROW_HEIGHT } from '@/design/text-viewer';
import type { TextViewerModel } from '@/features/files/text-viewer/use-text-viewer-model';
import type { useTextViewerViewport } from '@/features/files/text-viewer/use-text-viewer-viewport';
import type { SearchHighlightRange } from '@/app/components/ui/search-highlight';
import type { TextPageLine } from '@/features/files/text-viewer/types';
import type { KeyboardEvent } from 'react';

interface Props {
  encoding: string;
  isTruncated?: boolean;
  model: TextViewerModel;
  viewport: ReturnType<typeof useTextViewerViewport>;
  gutterWidth: number;
  rows: Array<{ line: TextPageLine; rowIndex: number; ranges: SearchHighlightRange[]; active: boolean }>;
  onSearchKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
}

export function TextViewerView({ encoding, isTruncated, model, viewport, gutterWidth, rows, onSearchKeyDown }: Props) {
  const { t } = useTranslation();
  const count = viewport.range.end - viewport.range.start;
  return (
    <div className="flex h-full min-h-0 flex-col" data-slot="text-viewer">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b bg-forensics-panel px-3 py-1.5 text-[11px]">
        <div className="flex min-w-0 flex-1 items-center gap-2 text-forensics-muted">
          <ActionIcon name="file" className="size-3" />
          <span>{encoding}</span>
          <span>{t('textViewer.lineCount', { count: model.totalLines })}</span>
          {isTruncated ? <span>{t('textViewer.truncated')}</span> : null}
          {model.totalLines >= 1000 ? <span className="hidden min-w-0 truncate xl:inline" role="status">
            {t('textViewer.largeContentMode', { start: model.page.startLine.toLocaleString(), end: model.page.endLine.toLocaleString(), rendered: count.toLocaleString() })}
          </span> : null}
        </div>
        <div role="search" className="flex items-center gap-1" aria-label={t('textViewer.searchLabel')}>
          <ActionIcon name="search" className="size-3 text-forensics-muted" />
          <Input value={model.query} onChange={(event) => model.setQuery(event.target.value)}
            data-text-search-input="true"
            onKeyDown={onSearchKeyDown}
            placeholder={t('textViewer.searchPlaceholder')} aria-label={t('textViewer.searchLabel')}
            inputSize="inline" className="w-32" />
          {model.query ? <span role="status" aria-label={t('textViewer.matchesLabel')} className="whitespace-nowrap px-1 font-mono text-forensics-ink">
            {t('textViewer.matches', { current: model.activeMatchIndex + 1, total: model.matchCount })}
          </span> : null}
          <IconButton label={t('textViewer.previousMatch')} size="iconXs" disabled={model.matchCount === 0} onClick={() => model.moveMatch(-1)}><ActionIcon name="chevron-up" /></IconButton>
          <IconButton label={t('textViewer.nextMatch')} size="iconXs" disabled={model.matchCount === 0} onClick={() => model.moveMatch(1)}><ActionIcon name="chevron-down" /></IconButton>
          {model.query ? <IconButton label={t('textViewer.clearSearch')} size="iconXs" onClick={() => model.setQuery('')}><ActionIcon name="close" /></IconButton> : null}
        </div>
        {model.pageCount > 1 ? <div className="flex items-center gap-1 text-forensics-muted">
          <IconButton label={t('textViewer.previousPage')} size="iconXs" disabled={model.pageNumber === 0} onClick={() => model.movePage(-1)}><ActionIcon name="chevron-left" /></IconButton>
          <span className="whitespace-nowrap font-mono">{t('textViewer.pageLabel', { current: model.pageNumber + 1, total: model.pageCount })}</span>
          <IconButton label={t('textViewer.nextPage')} size="iconXs" disabled={model.pageNumber === model.pageCount - 1} onClick={() => model.movePage(1)}><ActionIcon name="chevron-right" /></IconButton>
        </div> : null}
      </div>
      <ScrollArea className="min-h-0 flex-1 overflow-hidden bg-forensics-surface" viewportRef={viewport.viewportRef}
        viewportTestId="text-scroll-container" viewportProps={{ onScroll: viewport.onScroll }} showHorizontalScrollbar>
        <div className="relative min-w-full font-mono text-[11px] leading-[18px]" style={{ height: model.page.lines.length * TEXT_ROW_HEIGHT }}>
          <div data-testid="text-visible-window" className="absolute left-0 min-w-full w-max" style={{ top: viewport.range.start * TEXT_ROW_HEIGHT }}>
            {rows.map(({ line, rowIndex, ranges, active }) => {
              return <div key={`${line.startOffset}:${rowIndex}`} data-line-number={line.lineNumber}
                data-active={active ? 'true' : undefined} className="text-viewer-row flex min-w-full"
                style={{ height: TEXT_ROW_HEIGHT }}>
                <div data-testid="text-line-number" className="text-viewer-line-number sticky left-0 z-10 shrink-0 select-none border-r border-forensics-border-light bg-forensics-panel px-2 text-right text-forensics-muted-light"
                  style={{ width: gutterWidth }}>{line.continuation ? '...' : line.lineNumber}</div>
                <div className="min-w-0 whitespace-pre px-3"><span data-testid="text-line-content"><SearchHighlight text={line.content} ranges={ranges} /></span></div>
              </div>;
            })}
          </div>
        </div>
      </ScrollArea>
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-t bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted" aria-live="polite">
        <span>{model.query ? t('textViewer.status.search', { query: model.query }) : t('textViewer.status.ready')}</span>
        {model.query && model.matchCount === 0 ? <span>{t('textViewer.noMatches')}</span> : null}
        {model.query && model.position ? <span>{t('textViewer.position', model.position)}</span> : null}
        {isTruncated ? <span>{t('textViewer.previewScope')}</span> : null}
      </div>
    </div>
  );
}
