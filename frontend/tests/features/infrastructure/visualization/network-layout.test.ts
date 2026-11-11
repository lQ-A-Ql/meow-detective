import { describe, expect, it } from 'vitest';
import {
  hitNetworkNode,
  layoutNetworkNodes,
  networkCanvasHeight,
  networkCanvasMetrics,
  type NetworkPoint,
} from '@/features/infrastructure/visualization/network-layout';
import type { InfrastructureGraphNode } from '@/types/models';

const node = (id: string, kind: string): InfrastructureGraphNode => ({
  id,
  domain: 'environment',
  kind,
  name: id,
  status: 'ready',
  confidence: 'candidate',
  provenanceJson: '{}',
});

describe('networkCanvasHeight', () => {
  it('never drops below the minimum canvas height for zero or a single row of nodes', () => {
    expect(networkCanvasHeight([])).toBe(networkCanvasMetrics.minimumHeight);
    expect(networkCanvasHeight([node('a', 'physical_host'), node('b', 'pve'), node('c', 'virtual_machine')]))
      .toBe(networkCanvasMetrics.minimumHeight);
  });

  it('grows with one row per three nodes', () => {
    const nodes = Array.from({ length: 30 }, (_, index) => node(`n-${index}`, 'physical_host'));

    // Ten rows of (nodeHeight + rowGap) plus the fixed canvas padding.
    expect(networkCanvasHeight(nodes)).toBe(10 * (networkCanvasMetrics.nodeHeight + networkCanvasMetrics.rowGap) + 96);
    expect(networkCanvasHeight(nodes)).toBe(796);
  });
});

describe('layoutNetworkNodes', () => {
  it('returns an empty map for an empty graph', () => {
    expect(layoutNetworkNodes([], 900, 600).size).toBe(0);
  });

  it('places hosts, infrastructure services and workload systems in three lanes', () => {
    const points = layoutNetworkNodes([
      node('host-a', 'physical_host'),
      node('host-b', 'physical_host'),
      node('pve', 'pve'),
      node('ceph', 'ceph_cluster'),
      node('k8s', 'kubernetes'),
      node('vm-1', 'virtual_machine'),
      node('os-1', 'os_instance'),
    ], 900, 600);

    // laneWidth = 300; nodes are centered inside each lane.
    expect(points.get('host-a')).toEqual({ x: 150, y: 72, width: 244, height: 52 });
    expect(points.get('host-b')?.y).toBe(72 + networkCanvasMetrics.nodeHeight + networkCanvasMetrics.rowGap);
    expect(points.get('pve')?.x).toBe(450);
    expect(points.get('ceph')?.x).toBe(450);
    expect(points.get('k8s')?.x).toBe(450);
    expect(points.get('vm-1')?.x).toBe(750);
    expect(points.get('os-1')?.x).toBe(750);
    expect(points.get('os-1')?.y).toBe(72 + networkCanvasMetrics.nodeHeight + networkCanvasMetrics.rowGap);
  });

  it('centers unrecognized node kinds and clamps them above the canvas floor', () => {
    const leftovers = Array.from({ length: 9 }, (_, index) => node(`log-${index}`, 'container_log'));
    const points = layoutNetworkNodes(leftovers, 900, 600);

    expect(points.get('log-0')).toMatchObject({ x: 450, y: 72, width: 244, height: 52 });
    expect(points.get('log-7')?.y).toBe(600 - 40);
    expect(points.get('log-8')?.y).toBe(600 - 40);
  });

  it('keeps a 180px minimum node width on a narrow canvas', () => {
    const points = layoutNetworkNodes([node('host-a', 'physical_host')], 300, 460);

    expect(points.get('host-a')?.width).toBe(180);
  });
});

describe('hitNetworkNode', () => {
  const points = new Map<string, NetworkPoint>([
    ['host-a', { x: 150, y: 72, width: 200, height: 52 }],
    ['vm-1', { x: 450, y: 72, width: 200, height: 52 }],
  ]);

  it('is inclusive on every node boundary', () => {
    expect(hitNetworkNode(points, 150, 72)).toBe('host-a');
    expect(hitNetworkNode(points, 50, 46)).toBe('host-a');
    expect(hitNetworkNode(points, 250, 98)).toBe('host-a');
    expect(hitNetworkNode(points, 350, 46)).toBe('vm-1');
  });

  it('returns undefined outside every node or for an empty layout', () => {
    expect(hitNetworkNode(points, 49, 72)).toBeUndefined();
    expect(hitNetworkNode(points, 150, 45)).toBeUndefined();
    expect(hitNetworkNode(points, 300, 72)).toBeUndefined();
    expect(hitNetworkNode(new Map(), 150, 72)).toBeUndefined();
  });

  it('returns the first inserted node when two points overlap', () => {
    const overlapping = new Map<string, NetworkPoint>([
      ['first', { x: 100, y: 100, width: 100, height: 50 }],
      ['second', { x: 100, y: 100, width: 100, height: 50 }],
    ]);

    expect(hitNetworkNode(overlapping, 100, 100)).toBe('first');
  });
});
