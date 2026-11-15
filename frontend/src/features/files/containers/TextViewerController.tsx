import { useMemo } from 'react';
import { TextViewerView } from '@/components/viewers/TextViewerView';
import { useTextViewerModel } from '../text-viewer/use-text-viewer-model';
import { useTextViewerViewport } from '../text-viewer/use-text-viewer-viewport';
import type { TextViewerProps } from '../text-viewer/types';
import { searchRangesForLine } from '../text-viewer/text-search';
import {
  MAX_SYNTAX_PAGE_LENGTH,
  indexSyntaxRanges,
  languageForExtension,
  syntaxRangesForLine,
} from '../text-viewer/syntax-highlight';
import { useTextSearchKeyboard } from '../text-viewer/use-text-search-keyboard';

export function TextViewerController(props: TextViewerProps) {
  const model = useTextViewerModel(props);
  const gutterWidth = Math.max(String(model.page.endLine).length, 4) * 8 + 16;
  const viewport = useTextViewerViewport(model.page.lines, model.scrollRequest, gutterWidth);
  const pageStartOffset = model.page.lines[0]?.startOffset ?? 0;
  const pageEndOffset = model.page.lines[model.page.lines.length - 1]?.endOffset ?? pageStartOffset;
  const language = languageForExtension(props.extension);
  const pageText = useMemo(
    () => props.content.slice(pageStartOffset, pageEndOffset),
    [props.content, pageStartOffset, pageEndOffset],
  );
  const syntax = useMemo(() => (language ? indexSyntaxRanges(pageText, language) : undefined), [language, pageText]);
  const rows = viewport.visibleLines.map((line, offset) => {
    const ranges = searchRangesForLine(model.index, line.startOffset, line.endOffset, model.activeMatchIndex);
    const syntaxRanges = syntaxRangesForLine(syntax, line.startOffset, line.endOffset, pageStartOffset);
    return {
      line,
      rowIndex: viewport.range.start + offset,
      ranges,
      syntax: syntaxRanges,
      active: ranges.some((range) => range.active),
    };
  });
  const onSearchKeyDown = useTextSearchKeyboard(model, viewport.viewportRef);
  return (
    <TextViewerView
      content={props.content}
      encoding={props.encoding}
      isTruncated={props.isTruncated}
      syntaxDisabled={language !== undefined && pageText.length > MAX_SYNTAX_PAGE_LENGTH}
      model={model}
      viewport={viewport}
      gutterWidth={gutterWidth}
      rows={rows}
      onSearchKeyDown={onSearchKeyDown}
    />
  );
}
