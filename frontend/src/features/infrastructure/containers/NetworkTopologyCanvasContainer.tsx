import { useNetworkTopologyCanvasModel } from '../use-network-topology-canvas-model';
import { NetworkTopologyCanvasView } from '../components/NetworkTopologyCanvasView';
export function NetworkTopologyCanvas(props: Parameters<typeof useNetworkTopologyCanvasModel>[0]) { return <NetworkTopologyCanvasView model={useNetworkTopologyCanvasModel(props)} />; }
