import type { TFunction } from 'i18next';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { resolveInfrastructureVisualizationPalette, type InfrastructureVisualizationPalette } from '@/design/visualization-tokens';
import { INFRASTRUCTURE_DOMAIN_ORDER, type InfrastructureDomain } from '../types';
import { kindLabel } from '../logic/labels';
import { topologyCanvasMetrics, type CanvasPoint } from './topology-layout';

export function drawInfrastructureTopology(
  context: CanvasRenderingContext2D,
  element: Element,
  size: { width: number; height: number },
  domains: Record<InfrastructureDomain, InfrastructureGraphNode[]>,
  edges: InfrastructureGraphEdge[],
  positions: Map<string, CanvasPoint>,
  selectedId: string | undefined,
  t: TFunction,
) {
  const palette = resolveInfrastructureVisualizationPalette(element);
  context.clearRect(0, 0, size.width, size.height);
  context.fillStyle = palette.canvas;
  context.fillRect(0, 0, size.width, size.height);
  drawLanes(context, palette, size, domains, t);
  drawEdges(context, palette, edges, positions);
  drawNodes(context, palette, domains, positions, selectedId, t);
}

function drawLanes(
  context: CanvasRenderingContext2D,
  palette: InfrastructureVisualizationPalette,
  size: { width: number; height: number },
  domains: Record<InfrastructureDomain, InfrastructureGraphNode[]>,
  t: TFunction,
) {
  const laneWidth = size.width / INFRASTRUCTURE_DOMAIN_ORDER.length;
  INFRASTRUCTURE_DOMAIN_ORDER.forEach((domain, lane) => {
    context.fillStyle = palette.lane;
    context.fillRect(lane * laneWidth + 4, 4, laneWidth - 8, size.height - 8);
    context.fillStyle = palette.text;
    context.font = `${topologyCanvasMetrics.headingFontSize}px ${palette.fontFamily}`;
    context.fillText(t(`infrastructure.domains.${domain}`), lane * laneWidth + 16, 25);
    context.fillStyle = palette.muted;
    context.font = `${topologyCanvasMetrics.countFontSize}px ${palette.fontFamily}`;
    context.fillText(`${domains[domain].length}`, (lane + 1) * laneWidth - 28, 25);
  });
}

function drawEdges(
  context: CanvasRenderingContext2D,
  palette: InfrastructureVisualizationPalette,
  edges: InfrastructureGraphEdge[],
  positions: Map<string, CanvasPoint>,
) {
  edges.forEach((edge) => {
    const source = positions.get(edge.sourceId);
    const target = positions.get(edge.targetId);
    if (!source || !target || source.domain === target.domain) return;
    const forward = INFRASTRUCTURE_DOMAIN_ORDER.indexOf(source.domain)
      < INFRASTRUCTURE_DOMAIN_ORDER.indexOf(target.domain);
    const startX = forward ? source.x + source.width / 2 : source.x - source.width / 2;
    const endX = forward ? target.x - target.width / 2 : target.x + target.width / 2;
    context.strokeStyle = palette.edge;
    context.lineWidth = 1.4;
    context.beginPath();
    context.moveTo(startX, source.y);
    context.bezierCurveTo((startX + endX) / 2, source.y, (startX + endX) / 2, target.y, endX, target.y);
    context.stroke();
    drawArrow(context, endX, target.y, palette.edge, forward ? 1 : -1);
  });
}

function drawNodes(
  context: CanvasRenderingContext2D,
  palette: InfrastructureVisualizationPalette,
  domains: Record<InfrastructureDomain, InfrastructureGraphNode[]>,
  positions: Map<string, CanvasPoint>,
  selectedId: string | undefined,
  t: TFunction,
) {
  INFRASTRUCTURE_DOMAIN_ORDER.forEach((domain) => domains[domain].forEach((node) => {
    const point = positions.get(node.id);
    if (!point) return;
    const selected = selectedId === node.id;
    context.fillStyle = palette[domain];
    context.strokeStyle = selected ? palette.selected : palette.border;
    context.lineWidth = selected ? 3 : 1;
    context.beginPath();
    context.roundRect(
      point.x - point.width / 2,
      point.y - point.height / 2,
      point.width,
      point.height,
      topologyCanvasMetrics.cornerRadius,
    );
    context.fill();
    context.stroke();
    context.fillStyle = selected ? palette.selected : palette.muted;
    context.beginPath();
    context.arc(point.x - point.width / 2 + topologyCanvasMetrics.nodeIconOffset, point.y, 5, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = palette.text;
    context.font = `${selected ? '600 ' : ''}${topologyCanvasMetrics.labelFontSize}px ${palette.fontFamily}`;
    context.fillText(trimLabel(node.name, point.width - 34), point.x - point.width / 2 + topologyCanvasMetrics.nodeTextOffset, point.y - 2);
    context.fillStyle = palette.muted;
    context.font = `${topologyCanvasMetrics.detailFontSize}px ${palette.fontFamily}`;
    context.fillText(kindLabel(node.kind, t), point.x - point.width / 2 + topologyCanvasMetrics.nodeTextOffset, point.y + 13);
  }));
}

function drawArrow(context: CanvasRenderingContext2D, x: number, y: number, color: string, direction: 1 | -1) {
  context.fillStyle = color;
  context.beginPath();
  context.moveTo(x, y);
  context.lineTo(x - direction * topologyCanvasMetrics.edgeArrowLength, y - topologyCanvasMetrics.edgeArrowHalfHeight);
  context.lineTo(x - direction * topologyCanvasMetrics.edgeArrowLength, y + topologyCanvasMetrics.edgeArrowHalfHeight);
  context.closePath();
  context.fill();
}

function trimLabel(value: string, maxWidth: number) {
  if (value.length * 6 <= maxWidth) return value;
  return `${value.slice(0, Math.max(1, Math.floor(maxWidth / 6) - 1))}…`;
}
