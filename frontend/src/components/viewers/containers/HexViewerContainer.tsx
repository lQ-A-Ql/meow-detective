import { useHexViewerModel } from '../../../hooks/use-hex-viewer-model';
import { HexViewerView } from '../HexViewerView';
export function HexViewer(props: Parameters<typeof useHexViewerModel>[0]) { return <HexViewerView model={useHexViewerModel(props)} />; }
