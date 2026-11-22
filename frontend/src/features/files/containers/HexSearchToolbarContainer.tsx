import { HexSearchToolbar } from '@/components/preview/HexSearchToolbar';
import { useHexSearchToolbarModel, type HexSearchToolbarOptions } from '../hooks/use-hex-search-toolbar-model';

export function HexSearchToolbarContainer(props: HexSearchToolbarOptions) {
  return <HexSearchToolbar model={useHexSearchToolbarModel(props)} />;
}
