import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import type { TFunction } from 'i18next';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { CanvasSurface } from '@/app/components/ui/canvas-surface';
import { drawNetworkTopology } from '../visualization/draw-network-topology';
import { hitNetworkNode, layoutNetworkNodes, networkCanvasHeight } from '../visualization/network-layout';

export function NetworkTopologyCanvas({ nodes, edges, selectedId, onSelect, t }: {
  nodes: InfrastructureGraphNode[];
  edges: InfrastructureGraphEdge[];
  selectedId?: string;
  onSelect: (id: string) => void;
  t: TFunction;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 920, height: networkCanvasHeight(nodes) });
  const points = useMemo(() => layoutNetworkNodes(nodes, size.width, size.height), [nodes, size.height, size.width]);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const update = () => setSize({ width: Math.max(760, element.clientWidth), height: networkCanvasHeight(nodes) });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [nodes]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    context.scale(ratio, ratio);
    drawNetworkTopology(context, canvas, size, nodes, edges, points, selectedId, t);
  }, [edges, nodes, points, selectedId, size, t]);

  const handleClick = (event: MouseEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    onSelect(hitNetworkNode(points, ((event.clientX - rect.left) / rect.width) * size.width, ((event.clientY - rect.top) / rect.height) * size.height) ?? '');
  };

  return <div ref={containerRef}><CanvasSurface ref={canvasRef} className="h-auto min-w-[760px] cursor-pointer" onClick={handleClick} role="img" aria-label={t('infrastructure.networkTopology.canvasLabel')} /></div>;
}
