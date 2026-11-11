import { useCallback, useState } from 'react';
import type { FilePreviewTabsProps } from '@/components/preview/FilePreviewTabs';
export type HexPreviewProps = Pick<
  FilePreviewTabsProps,
  'viewer' | 'onHexJumpInputChange' | 'onHexJump' | 'onLoadNextHexRange' | 'onLoadPreviousHexRange'
>;
export function useHexPreviewModel(props: HexPreviewProps) {
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
    inspectorExpanded,
    handleNeedMoreRange,
    toggleInspector: () => setDisclosure({ key, expanded: !inspectorExpanded }),
  };
}
