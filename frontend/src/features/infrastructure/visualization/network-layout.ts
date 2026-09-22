import type { InfrastructureGraphNode } from '@/types/models';

export interface NetworkPoint {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const networkCanvasMetrics = {
  minimumWidth: 760,
  minimumHeight: 460,
  nodeHeight: 52,
  columnGap: 28,
  rowGap: 18,
} as const;

export function networkCanvasHeight(nodes: InfrastructureGraphNode[]) {
  const rows = Math.max(1, Math.ceil(nodes.length / 3));
  return Math.max(networkCanvasMetrics.minimumHeight, rows * (networkCanvasMetrics.nodeHeight + networkCanvasMetrics.rowGap) + 96);
}

export function layoutNetworkNodes(nodes: InfrastructureGraphNode[], width: number, height: number) {
  const columns = [
    nodes.filter((node) => node.kind === 'physical_host'),
    nodes.filter((node) => ['pve', 'ceph', 'ceph_cluster', 'kubernetes'].includes(node.kind)),
    nodes.filter((node) => ['virtual_machine', 'os_instance'].includes(node.kind)),
  ];
  const points = new Map<string, NetworkPoint>();
  const laneWidth = width / 3;
  columns.forEach((column, lane) => {
    column.forEach((node, index) => {
      points.set(node.id, {
        x: lane * laneWidth + laneWidth / 2,
        y: 72 + index * (networkCanvasMetrics.nodeHeight + networkCanvasMetrics.rowGap),
        width: Math.max(180, laneWidth - networkCanvasMetrics.columnGap * 2),
        height: networkCanvasMetrics.nodeHeight,
      });
    });
  });
  const leftovers = nodes.filter((node) => !points.has(node.id));
  leftovers.forEach((node, index) => {
    points.set(node.id, {
      x: width / 2,
      y: Math.min(height - 40, 72 + index * (networkCanvasMetrics.nodeHeight + networkCanvasMetrics.rowGap)),
      width: Math.max(180, laneWidth - networkCanvasMetrics.columnGap * 2),
      height: networkCanvasMetrics.nodeHeight,
    });
  });
  return points;
}

export function hitNetworkNode(points: Map<string, NetworkPoint>, x: number, y: number) {
  return [...points.entries()].find(([, point]) => (
    x >= point.x - point.width / 2
      && x <= point.x + point.width / 2
      && y >= point.y - point.height / 2
      && y <= point.y + point.height / 2
  ))?.[0];
}
