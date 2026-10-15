import type { useNetworkTopologyCanvasModel } from '../use-network-topology-canvas-model';
import { CanvasSurface } from '@/app/components/ui/canvas-surface';

export function NetworkTopologyCanvasView({ model }: { model: ReturnType<typeof useNetworkTopologyCanvasModel> }) {
const { t, containerRef, canvasRef, handleClick } = model;



  return <div ref={containerRef}><CanvasSurface ref={canvasRef} className="h-auto min-w-[760px] cursor-pointer" onClick={handleClick} role="img" aria-label={t('infrastructure.networkTopology.canvasLabel')} /></div>;
}
