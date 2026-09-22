import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import type { TFunction } from 'i18next';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { CanvasSurface } from '@/app/components/ui/canvas-surface';
import { drawInfrastructureTopology } from '../visualization/draw-topology';
import { hitTopologyNode, layoutTopologyNodes, topologyCanvasHeight } from '../visualization/topology-layout';
import type { InfrastructureDomain } from '../types';

export function TopologyCanvas({
  domains,
  edges,
  selectedId,
  onSelect,
  t,
}: {
  domains: Record<InfrastructureDomain, InfrastructureGraphNode[]>;
  edges: InfrastructureGraphEdge[];
  selectedId?: string;
  onSelect: (id: string) => void;
  t: TFunction;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 920, height: 560 });
  const positions = useMemo(() => layoutTopologyNodes(domains, size.width), [domains, size.width]);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const update = () => setSize({
      width: Math.max(760, element.clientWidth),
      height: topologyCanvasHeight(domains),
    });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [domains]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    const context = canvas.getContext('2d');
    if (!context) return;
    context.scale(ratio, ratio);
    drawInfrastructureTopology(context, canvas, size, domains, edges, positions, selectedId, t);
  }, [domains, edges, positions, selectedId, size, t]);

  const handleClick = (event: MouseEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / rect.width) * size.width;
    const y = ((event.clientY - rect.top) / rect.height) * size.height;
    onSelect(hitTopologyNode(positions, x, y) ?? '');
  };

  return (
    <div ref={containerRef}>
      <CanvasSurface
        ref={canvasRef}
        className="h-auto min-w-[760px] cursor-pointer"
        onClick={handleClick}
        role="img"
        aria-label={t('infrastructure.topologyCanvasLabel')}
      />
    </div>
  );
}
