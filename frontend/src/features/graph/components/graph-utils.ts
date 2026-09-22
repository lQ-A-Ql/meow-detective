import type { EdgeType, GraphEdge, GraphNode, NodeType } from '@/types/models';

export interface SimulationNode {
  id: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
}

export const NODE_COLORS: Record<NodeType, string> = {
  file: 'var(--forensics-graph-node-file)',
  artifact: 'var(--forensics-graph-node-artifact)',
  timelineEvent: 'var(--forensics-graph-node-timeline-event)',
  entity: 'var(--forensics-graph-node-entity)',
  lead: 'var(--forensics-graph-node-lead)',
  notebookEntry: 'var(--forensics-graph-node-notebook-entry)',
};

export const NODE_TYPE_KEYS: Record<NodeType, string> = {
  file: 'graph.nodeTypes.file',
  artifact: 'graph.nodeTypes.artifact',
  timelineEvent: 'graph.nodeTypes.timelineEvent',
  entity: 'graph.nodeTypes.entity',
  lead: 'graph.nodeTypes.lead',
  notebookEntry: 'graph.nodeTypes.notebookEntry',
};

export const EDGE_COLORS: Record<EdgeType, string> = {
  contains: 'var(--forensics-graph-edge-contains)',
  references: 'var(--forensics-graph-edge-references)',
  correlatesWith: 'var(--forensics-graph-edge-correlates)',
  derivesFrom: 'var(--forensics-graph-edge-derives)',
  precedes: 'var(--forensics-graph-edge-precedes)',
  cites: 'var(--forensics-graph-edge-cites)',
  annotates: 'var(--forensics-graph-edge-annotates)',
};

export const EDGE_TYPE_KEYS: Record<EdgeType, string> = {
  contains: 'graph.edgeTypes.contains',
  references: 'graph.edgeTypes.references',
  correlatesWith: 'graph.edgeTypes.correlatesWith',
  derivesFrom: 'graph.edgeTypes.derivesFrom',
  precedes: 'graph.edgeTypes.precedes',
  cites: 'graph.edgeTypes.cites',
  annotates: 'graph.edgeTypes.annotates',
};

export const ALL_EDGE_TYPES: EdgeType[] = [
  'contains',
  'references',
  'correlatesWith',
  'derivesFrom',
  'precedes',
  'cites',
  'annotates',
];

export function nodeTypeColor(nodeType: NodeType): string {
  return NODE_COLORS[nodeType] ?? 'var(--forensics-graph-node-fallback)';
}

export function edgeTypeColor(edgeType: EdgeType): string {
  return EDGE_COLORS[edgeType] ?? 'var(--forensics-graph-edge-fallback)';
}

export function buildNodeMap(nodes: GraphNode[]): Map<string, GraphNode> {
  return new Map(nodes.map((n) => [n.id, n]));
}

export function buildEdgeMap(edges: GraphEdge[]): Map<string, GraphEdge> {
  return new Map(edges.map((e) => [e.id, e]));
}

export function degreeMap(nodes: GraphNode[], edges: GraphEdge[]): Map<string, number> {
  const degrees = new Map<string, number>();
  for (const node of nodes) {
    degrees.set(node.id, 0);
  }
  for (const edge of edges) {
    degrees.set(edge.sourceId, (degrees.get(edge.sourceId) ?? 0) + 1);
    degrees.set(edge.targetId, (degrees.get(edge.targetId) ?? 0) + 1);
  }
  return degrees;
}

export function deterministicNodePosition(
  id: string,
  width: number,
  height: number,
  spread = 80,
): { x: number; y: number } {
  return {
    x: width / 2 + stableSignedUnit(`${id}:x`) * spread,
    y: height / 2 + stableSignedUnit(`${id}:y`) * spread,
  };
}

export function tickSimulation(
  positions: Map<string, SimulationNode>,
  edges: GraphEdge[],
  width: number,
  height: number,
  options: {
    repulsion?: number;
    springLength?: number;
    springStrength?: number;
    centerStrength?: number;
    damping?: number;
    maxSpeed?: number;
  } = {},
) {
  const {
    repulsion = 8000,
    springLength = 90,
    springStrength = 0.05,
    centerStrength = 0.01,
    damping = 0.6,
    maxSpeed = 12,
  } = options;

  const centerX = width / 2;
  const centerY = height / 2;
  const nodeList = Array.from(positions.values());

  // Repulsion
  for (let i = 0; i < nodeList.length; i += 1) {
    const a = nodeList[i];
    for (let j = i + 1; j < nodeList.length; j += 1) {
      const b = nodeList[j];
      let dx = a.x - b.x;
      let dy = a.y - b.y;
      let distSq = dx * dx + dy * dy;
      if (distSq === 0) {
        dx = stableSignedUnit(`${a.id}:${b.id}:x`) || 0.01;
        dy = stableSignedUnit(`${a.id}:${b.id}:y`) || 0.01;
        distSq = dx * dx + dy * dy;
      }
      const dist = Math.sqrt(distSq);
      const force = repulsion / distSq;
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      a.vx += fx;
      a.vy += fy;
      b.vx -= fx;
      b.vy -= fy;
    }
  }

  // Spring attraction along edges
  for (const edge of edges) {
    const source = positions.get(edge.sourceId);
    const target = positions.get(edge.targetId);
    if (!source || !target) continue;
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const dist = Math.sqrt(dx * dx + dy * dy) || 1;
    const force = (dist - springLength) * springStrength;
    const fx = (dx / dist) * force;
    const fy = (dy / dist) * force;
    source.vx += fx;
    source.vy += fy;
    target.vx -= fx;
    target.vy -= fy;
  }

  // Center gravity
  for (const node of nodeList) {
    node.vx += (centerX - node.x) * centerStrength;
    node.vy += (centerY - node.y) * centerStrength;

    // Clamp speed
    const speed = Math.sqrt(node.vx * node.vx + node.vy * node.vy);
    if (speed > maxSpeed) {
      node.vx = (node.vx / speed) * maxSpeed;
      node.vy = (node.vy / speed) * maxSpeed;
    }

    // Apply velocity with damping
    node.x += node.vx;
    node.y += node.vy;
    node.vx *= damping;
    node.vy *= damping;
  }
}

function stableSignedUnit(value: string): number {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0) / 0xffffffff - 0.5;
}

export function fitTransform(
  positions: Map<string, SimulationNode>,
  width: number,
  height: number,
  padding = 60,
): { x: number; y: number; k: number } {
  const nodes = Array.from(positions.values());
  if (nodes.length === 0) {
    return { x: 0, y: 0, k: 1 };
  }
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const n of nodes) {
    minX = Math.min(minX, n.x - n.radius);
    minY = Math.min(minY, n.y - n.radius);
    maxX = Math.max(maxX, n.x + n.radius);
    maxY = Math.max(maxY, n.y + n.radius);
  }
  const bboxW = Math.max(1, maxX - minX + padding * 2);
  const bboxH = Math.max(1, maxY - minY + padding * 2);
  const k = Math.min(width / bboxW, height / bboxH, 2);
  const x = (width - (maxX + minX) * k) / 2;
  const y = (height - (maxY + minY) * k) / 2;
  return { x, y, k };
}
