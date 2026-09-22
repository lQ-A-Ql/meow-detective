import type { InfrastructureGraphNode } from '@/types/models';
import { INFRASTRUCTURE_DOMAIN_ORDER, type InfrastructureDomain } from '../types';

export const topologyCanvasMetrics = {
  minimumWidth: 760,
  minimumHeight: 480,
  headerHeight: 62,
  rowSpacing: 72,
  nodeHeight: 48,
  laneInset: 14,
  nodeInset: 28,
  cornerRadius: 5,
  nodeIconOffset: 13,
  nodeTextOffset: 25,
  edgeArrowLength: 7,
  edgeArrowHalfHeight: 4,
  headingFontSize: 13,
  countFontSize: 10,
  labelFontSize: 11,
  detailFontSize: 9,
} as const;

export type CanvasPoint = {
  x: number;
  y: number;
  width: number;
  height: number;
  domain: InfrastructureDomain;
};

export function topologyCanvasHeight(domains: Record<InfrastructureDomain, InfrastructureGraphNode[]>) {
  const maxRows = Math.max(...INFRASTRUCTURE_DOMAIN_ORDER.map((domain) => domains[domain].length), 1);
  return Math.max(
    topologyCanvasMetrics.minimumHeight,
    topologyCanvasMetrics.headerHeight + maxRows * topologyCanvasMetrics.rowSpacing + 24,
  );
}

export function layoutTopologyNodes(
  domains: Record<InfrastructureDomain, InfrastructureGraphNode[]>,
  width: number,
) {
  const points = new Map<string, CanvasPoint>();
  const laneWidth = width / INFRASTRUCTURE_DOMAIN_ORDER.length;
  INFRASTRUCTURE_DOMAIN_ORDER.forEach((domain, lane) => {
    domains[domain].forEach((node, index) => {
      points.set(node.id, {
        x: lane * laneWidth + laneWidth / 2,
        y: topologyCanvasMetrics.headerHeight + index * topologyCanvasMetrics.rowSpacing,
        width: laneWidth - topologyCanvasMetrics.nodeInset,
        height: topologyCanvasMetrics.nodeHeight,
        domain,
      });
    });
  });
  return points;
}

export function hitTopologyNode(points: Map<string, CanvasPoint>, x: number, y: number) {
  return [...points.entries()].find(([, point]) => (
    x >= point.x - point.width / 2
    && x <= point.x + point.width / 2
    && y >= point.y - point.height / 2
    && y <= point.y + point.height / 2
  ))?.[0];
}
