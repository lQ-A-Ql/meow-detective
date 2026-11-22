import { useCallback, useMemo, useState } from 'react';
import { useHexSearch, hexPatternByteLength } from '@/features/files/hooks/use-hex-search';
import { useHexSearchToolbarModel } from '@/features/files/hooks/use-hex-search-toolbar-model';
import type { FilePreviewTabsProps } from '@/components/preview/FilePreviewTabs';
export type HexPreviewProps = Pick<
  FilePreviewTabsProps,
  'viewer' | 'onHexJumpInputChange' | 'onHexJump' | 'onLoadNextHexRange' | 'onLoadPreviousHexRange'
> & { fileId?: string };
export function useHexPreviewModel(props: HexPreviewProps) {
  const search = useHexSearch({ fileId: props.fileId, onJump: props.onHexJump });
  const toolbar = useHexSearchToolbarModel({
    fileId: props.fileId,
    state: search.state,
    onStart: (mode, pattern) => void search.start(mode, pattern),
    onCancel: () => void search.cancel(),
    onReset: () => void search.reset(),
    onPrevious: search.previous,
    onNext: search.next,
  });
  const matchLength = search.state ? hexPatternByteLength(search.state.mode, search.state.pattern) : undefined;
  const matches = search.state?.matches;
  const searchRanges = useMemo(() => {
    if (!matches || matchLength === undefined) return undefined;
    return matches.map((start) => ({ start, end: start + matchLength }));
  }, [matchLength, matches]);
  const activeMatchStart =
    search.state && search.state.activeIndex >= 0 ? search.state.matches[search.state.activeIndex] : undefined;
  const { onLoadNextHexRange, onLoadPreviousHexRange } = props;
  const key = props.viewer?.handle.handleId;
  const [disclosure, setDisclosure] = useState({ key, expanded: false });
  const inspectorExpanded = disclosure.key === key && disclosure.expanded;
  const handleNeedMoreRange = useCallback(
    (direction: 'previous' | 'next') => {
      if (direction === 'next') void onLoadNextHexRange();
      else void onLoadPreviousHexRange();
    },
    [onLoadNextHexRange, onLoadPreviousHexRange],
  );
  return {
    ...props,
    toolbar,
    searchRanges,
    activeMatchStart,
    inspectorExpanded,
    handleNeedMoreRange,
    toggleInspector: () => setDisclosure({ key, expanded: !inspectorExpanded }),
  };
}
