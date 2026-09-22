import type { TFunction } from 'i18next';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { resolveInfrastructureVisualizationPalette } from '@/design/visualization-tokens';
import { kindLabel } from '../logic/labels';
import type { NetworkPoint } from './network-layout';

export function drawNetworkTopology(
  context: CanvasRenderingContext2D,
  element: Element,
  size: { width: number; height: number },
  nodes: InfrastructureGraphNode[],
  edges: InfrastructureGraphEdge[],
  points: Map<string, NetworkPoint>,
  selectedId: string | undefined,
  t: TFunction,
) {
  const palette = resolveInfrastructureVisualizationPalette(element);
  context.clearRect(0, 0, size.width, size.height);
  context.fillStyle = palette.canvas;
  context.fillRect(0, 0, size.width, size.height);
  context.fillStyle = palette.text;
  context.font = `13px ${palette.fontFamily}`;
  context.fillText(t('infrastructure.networkTopology.hostNetwork'), 18, 28);
  context.fillStyle = palette.muted;
  context.font = `10px ${palette.fontFamily}`;
  context.fillText(t('infrastructure.networkTopology.hint'), 18, 46);

  for (const edge of edges) {
    const source = points.get(edge.sourceId);
    const target = points.get(edge.targetId);
    if (!source || !target) continue;
    context.strokeStyle = palette.edge;
    context.lineWidth = edge.relationKind === 'peer' ? 1 : 1.6;
    context.setLineDash(edge.relationKind === 'peer' ? [5, 4] : []);
    context.beginPath();
    context.moveTo(source.x, source.y);
    context.bezierCurveTo((source.x + target.x) / 2, source.y, (source.x + target.x) / 2, target.y, target.x, target.y);
    context.stroke();
    context.setLineDash([]);
  }

  for (const node of nodes) {
    const point = points.get(node.id);
    if (!point) continue;
    const selected = selectedId === node.id;
    context.fillStyle = palette[node.domain];
    context.strokeStyle = selected ? palette.selected : palette.border;
    context.lineWidth = selected ? 3 : 1;
    context.beginPath();
    context.roundRect(point.x - point.width / 2, point.y - point.height / 2, point.width, point.height, 5);
    context.fill();
    context.stroke();
    context.fillStyle = palette.text;
    context.font = `${selected ? '600 ' : ''}11px ${palette.fontFamily}`;
    context.fillText(trim(node.name, point.width - 26), point.x - point.width / 2 + 14, point.y - 3);
    context.fillStyle = palette.muted;
    context.font = `9px ${palette.fontFamily}`;
    context.fillText(kindLabel(node.kind, t), point.x - point.width / 2 + 14, point.y + 14);
  }
}

function trim(value: string, width: number) {
  if (value.length * 6 <= width) return value;
  return `${value.slice(0, Math.max(1, Math.floor(width / 6) - 1))}…`;
}
