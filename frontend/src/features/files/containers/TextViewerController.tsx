import { TextViewerView } from '@/components/viewers/TextViewerView';
import { useTextViewerModel } from '../text-viewer/use-text-viewer-model';
import { useTextViewerViewport } from '../text-viewer/use-text-viewer-viewport';
import type { TextViewerProps } from '../text-viewer/types';
import { searchRangesForLine } from '../text-viewer/text-search';
import { useTextSearchKeyboard } from '../text-viewer/use-text-search-keyboard';

export function TextViewerController(props: TextViewerProps) {
  const model = useTextViewerModel(props);
  const gutterWidth = Math.max(String(model.page.endLine).length, 4) * 8 + 16;
  const viewport = useTextViewerViewport(model.page.lines, model.scrollRequest, gutterWidth);
  const rows = viewport.visibleLines.map((line, offset) => {
    const ranges = searchRangesForLine(model.index, line.startOffset, line.endOffset, model.activeMatchIndex);
    return { line, rowIndex: viewport.range.start + offset, ranges, active: ranges.some((range) => range.active) };
  });
  const onSearchKeyDown = useTextSearchKeyboard(model, viewport.viewportRef);
  return <TextViewerView encoding={props.encoding} isTruncated={props.isTruncated} model={model}
    viewport={viewport} gutterWidth={gutterWidth} rows={rows} onSearchKeyDown={onSearchKeyDown} />;
}
