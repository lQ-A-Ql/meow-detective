import { Minus, Plus, RotateCcw, Target } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { GraphEdge, GraphNode, NodeType } from '@/types/models';
import {
  ALL_EDGE_TYPES,
  degreeMap,
  edgeTypeColor,
  fitTransform,
  nodeTypeColor,
  NODE_LABELS,
  tickSimulation,
  type SimulationNode,
} from './graph-utils';

interface ForceGraphProps {
  nodes: GraphNode[];
  edges: GraphEdge[];
  selectedNodeId?: string;
  selectedEdgeId?: string;
  onNodeClick?: (node: GraphNode) => void;
  onNodeDoubleClick?: (node: GraphNode) => void;
  onEdgeClick?: (edge: GraphEdge) => void;
  onBackgroundClick?: () => void;
  width: number;
  height: number;
  running?: boolean;
}

export function ForceGraph({
  nodes,
  edges,
  selectedNodeId,
  selectedEdgeId,
  onNodeClick,
  onNodeDoubleClick,
  onEdgeClick,
  onBackgroundClick,
  width,
  height,
  running = true,
}: ForceGraphProps) {
  const containerRef = useRef<SVGSVGElement>(null);
  const [positions, setPositions] = useState<Map<string, SimulationNode>>(new Map());
  const [transform, setTransform] = useState({ x: 0, y: 0, k: 1 });
  const initialFitRef = useRef(false);
  const dragNodeIdRef = useRef<string | null>(null);
  const isPanningRef = useRef(false);
  const lastPointerRef = useRef({ x: 0, y: 0 });

  const degrees = degreeMap(nodes, edges);

  // Initialize / merge positions when nodes change.
  useEffect(() => {
    if (width === 0 || height === 0) return;
    setPositions((prev) => {
      const next = new Map(prev);
      let changed = false;
      const nodeIds = new Set(nodes.map((n) => n.id));
      for (const node of nodes) {
        if (!next.has(node.id)) {
          const degree = degrees.get(node.id) ?? 0;
          const radius = 6 + Math.sqrt(degree) * 2;
          next.set(node.id, {
            id: node.id,
            x: width / 2 + (Math.random() - 0.5) * 80,
            y: height / 2 + (Math.random() - 0.5) * 80,
            vx: 0,
            vy: 0,
            radius,
          });
          changed = true;
        }
      }
      for (const [id] of next) {
        if (!nodeIds.has(id)) {
          next.delete(id);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [nodes, edges, width, height]);

  // Fit view once after positions are first populated.
  useEffect(() => {
    if (positions.size > 0 && !initialFitRef.current && width > 0 && height > 0) {
      setTransform(fitTransform(positions, width, height));
      initialFitRef.current = true;
    }
  }, [positions, width, height]);

  // Simulation loop.
  useEffect(() => {
    if (!running || positions.size === 0 || width === 0 || height === 0) return;
    let raf = 0;
    const loop = () => {
      setPositions((prev) => {
        const next = new Map(prev);
        tickSimulation(next, edges, width, height);
        return next;
      });
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [running, positions.size, edges, width, height]);

  function screenToWorld(sx: number, sy: number) {
    return {
      x: (sx - transform.x) / transform.k,
      y: (sy - transform.y) / transform.k,
    };
  }

  function zoomAround(delta: number, sx: number, sy: number) {
    const factor = delta > 0 ? 0.9 : 1.1;
    const newK = Math.min(Math.max(transform.k * factor, 0.1), 5);
    const world = screenToWorld(sx, sy);
    setTransform({
      x: sx - world.x * newK,
      y: sy - world.y * newK,
      k: newK,
    });
  }

  function handlePointerDown(event: React.PointerEvent<SVGSVGElement>) {
    const target = event.target as Element;
    if (target.closest('[data-node]')) return;
    if (event.button !== 0) return;
    isPanningRef.current = true;
    lastPointerRef.current = { x: event.clientX, y: event.clientY };
    (event.currentTarget as SVGSVGElement).setPointerCapture(event.pointerId);
  }

  function handlePointerMove(event: React.PointerEvent<SVGSVGElement>) {
    const nodeId = dragNodeIdRef.current;
    if (nodeId) {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const world = screenToWorld(event.clientX - rect.left, event.clientY - rect.top);
      setPositions((prev) => {
        const next = new Map(prev);
        const n = next.get(nodeId);
        if (n) {
          n.x = world.x;
          n.y = world.y;
          n.vx = 0;
          n.vy = 0;
        }
        return next;
      });
      return;
    }
    if (!isPanningRef.current) return;
    const dx = event.clientX - lastPointerRef.current.x;
    const dy = event.clientY - lastPointerRef.current.y;
    lastPointerRef.current = { x: event.clientX, y: event.clientY };
    setTransform((t) => ({ ...t, x: t.x + dx, y: t.y + dy }));
  }

  function handlePointerUp(event: React.PointerEvent<SVGSVGElement>) {
    dragNodeIdRef.current = null;
    isPanningRef.current = false;
    try {
      (event.currentTarget as SVGSVGElement).releasePointerCapture(event.pointerId);
    } catch {
      // capture may already be released
    }
  }

  function handleWheel(event: React.WheelEvent<SVGSVGElement>) {
    event.preventDefault();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const sx = event.clientX - rect.left;
    const sy = event.clientY - rect.top;
    zoomAround(event.deltaY, sx, sy);
  }

  function handleNodePointerDown(event: React.PointerEvent<SVGCircleElement>, node: GraphNode) {
    event.stopPropagation();
    dragNodeIdRef.current = node.id;
    lastPointerRef.current = { x: event.clientX, y: event.clientY };
    (event.currentTarget as SVGCircleElement).setPointerCapture(event.pointerId);
    onNodeClick?.(node);
  }

  function handleNodePointerUp(event: React.PointerEvent<SVGCircleElement>) {
    dragNodeIdRef.current = null;
    try {
      (event.currentTarget as SVGCircleElement).releasePointerCapture(event.pointerId);
    } catch {
      // ignore
    }
  }

  const showAllLabels = nodes.length <= 40;

  return (
    <svg
      ref={containerRef}
      className="h-full w-full cursor-grab active:cursor-grabbing"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerLeave={handlePointerUp}
      onWheel={handleWheel}
      onClick={onBackgroundClick}
      role="img"
      aria-label="关系图谱力导向图"
    >
      <defs>
        {ALL_EDGE_TYPES.map((type) => (
          <marker
            key={type}
            id={`arrow-${type}`}
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill={edgeTypeColor(type)} />
          </marker>
        ))}
      </defs>
      <g transform={`translate(${transform.x} ${transform.y}) scale(${transform.k})`}>
        {/* Edges */}
        {edges.map((edge) => {
          const source = positions.get(edge.sourceId);
          const target = positions.get(edge.targetId);
          if (!source || !target) return null;
          const isSelected = selectedEdgeId === edge.id;
          return (
            <g key={edge.id} data-edge>
              <line
                x1={source.x}
                y1={source.y}
                x2={target.x}
                y2={target.y}
                stroke={edgeTypeColor(edge.edgeType)}
                strokeWidth={isSelected ? 2.5 : 1.5}
                strokeOpacity={isSelected ? 1 : 0.7}
                markerEnd={`url(#arrow-${edge.edgeType})`}
                className="cursor-pointer"
                onClick={(event) => {
                  event.stopPropagation();
                  onEdgeClick?.(edge);
                }}
              />
              {isSelected ? (
                <line
                  x1={source.x}
                  y1={source.y}
                  x2={target.x}
                  y2={target.y}
                  stroke="#111"
                  strokeWidth={5}
                  strokeOpacity={0.1}
                  pointerEvents="none"
                />
              ) : null}
            </g>
          );
        })}

        {/* Nodes */}
        {nodes.map((node) => {
          const pos = positions.get(node.id);
          if (!pos) return null;
          const isSelected = selectedNodeId === node.id;
          const degree = degrees.get(node.id) ?? 0;
          const showLabel = showAllLabels || isSelected || degree > 2;
          return (
            <g
              key={node.id}
              transform={`translate(${pos.x} ${pos.y})`}
              data-node
              onDoubleClick={(event) => {
                event.stopPropagation();
                onNodeDoubleClick?.(node);
              }}
            >
              {isSelected ? (
                <circle
                  r={pos.radius + 4}
                  fill="none"
                  stroke="#111"
                  strokeWidth={2}
                  pointerEvents="none"
                />
              ) : null}
              <circle
                r={pos.radius}
                fill={nodeTypeColor(node.nodeType)}
                stroke="#fff"
                strokeWidth={1.5}
                className="cursor-pointer"
                onPointerDown={(event) => handleNodePointerDown(event, node)}
                onPointerUp={handleNodePointerUp}
              />
              {showLabel ? (
                <text
                  y={pos.radius + 12}
                  textAnchor="middle"
                  className="pointer-events-none select-none fill-forensics-900"
                  style={{ fontSize: 9, fontFamily: 'monospace' }}
                >
                  {node.label || node.id}
                </text>
              ) : null}
              <title>{`${node.label || node.id} (${NODE_LABELS[node.nodeType as NodeType] ?? node.nodeType})`}</title>
            </g>
          );
        })}
      </g>
      <GraphOverlay
        onZoomIn={() => setTransform((t) => ({ ...t, k: Math.min(t.k * 1.2, 5) }))}
        onZoomOut={() => setTransform((t) => ({ ...t, k: Math.max(t.k / 1.2, 0.1) }))}
        onFit={() => setTransform(fitTransform(positions, width, height))}
        onReset={() => {
          initialFitRef.current = false;
          setPositions((prev) => {
            const next = new Map(prev);
            for (const n of next.values()) {
              n.x = width / 2 + (Math.random() - 0.5) * 80;
              n.y = height / 2 + (Math.random() - 0.5) * 80;
              n.vx = 0;
              n.vy = 0;
            }
            return next;
          });
        }}
      />
    </svg>
  );
}

function GraphOverlay({
  onZoomIn,
  onZoomOut,
  onFit,
  onReset,
}: {
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onReset: () => void;
}) {
  const btn =
    'pointer-events-auto flex h-7 w-7 items-center justify-center rounded border border-forensics-border bg-white text-forensics-muted shadow-sm hover:bg-forensics-hover hover:text-forensics-text';
  return (
    <g pointerEvents="none">
      <foreignObject x="10" y="10" width="36" height="164">
        <div className="flex flex-col gap-1.5">
          <button type="button" className={btn} onClick={onZoomIn} title="放大">
            <Plus size={14} />
          </button>
          <button type="button" className={btn} onClick={onZoomOut} title="缩小">
            <Minus size={14} />
          </button>
          <button type="button" className={btn} onClick={onFit} title="适应视图">
            <Target size={14} />
          </button>
          <button type="button" className={btn} onClick={onReset} title="重置布局">
            <RotateCcw size={14} />
          </button>
        </div>
      </foreignObject>
    </g>
  );
}
