import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ForceGraph } from '@/features/graph/visualization/ForceGraph';
import { deterministicNodePosition } from '@/features/graph/logic/graph-utils';
import type { EdgeType, GraphEdge, GraphNode } from '@/types/models';

function makeNode(id: string, label = id): GraphNode {
  return {
    id,
    caseId: 'case-1',
    nodeType: 'file',
    label,
    summary: '',
    tags: [],
    createdAt: '2026-06-14T00:00:00Z',
  };
}

function makeEdge(
  id: string,
  sourceId: string,
  targetId: string,
  edgeType: EdgeType = 'references',
): GraphEdge {
  return {
    id,
    caseId: 'case-1',
    sourceId,
    targetId,
    edgeType,
    createdAt: '2026-06-14T00:00:00Z',
  };
}

let frames: Map<number, FrameRequestCallback>;
let nextFrameId: number;

beforeEach(() => {
  frames = new Map();
  nextFrameId = 0;
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    nextFrameId += 1;
    frames.set(nextFrameId, callback);
    return nextFrameId;
  });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => {
    frames.delete(id);
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function takeFrame() {
  const next = frames.entries().next();
  if (next.done) {
    throw new Error('No animation frame was scheduled');
  }
  const [id, callback] = next.value;
  frames.delete(id);
  act(() => {
    callback(0);
  });
}

function viewport(graph: HTMLElement): SVGGElement {
  const element = graph.querySelector('g[transform]');
  if (!element) throw new Error('Missing viewport group');
  return element as SVGGElement;
}

function parseTransform(element: Element): { x: number; y: number; k: number } {
  const match = /translate\(([-\d.]+) ([-\d.]+)\) scale\(([-\d.]+)\)/.exec(
    element.getAttribute('transform') ?? '',
  );
  if (!match) throw new Error(`Unexpected transform: ${element.getAttribute('transform')}`);
  return { x: Number(match[1]), y: Number(match[2]), k: Number(match[3]) };
}

function renderGraph(overrides: {
  nodes?: GraphNode[];
  edges?: GraphEdge[];
  selectedNodeId?: string;
  selectedEdgeId?: string;
  onNodeClick?: (node: GraphNode) => void;
  onNodeDoubleClick?: (node: GraphNode) => void;
  onEdgeClick?: (edge: GraphEdge) => void;
  onBackgroundClick?: () => void;
  width?: number;
  height?: number;
  running?: boolean;
} = {}) {
  const props = {
    nodes: overrides.nodes ?? [],
    edges: overrides.edges ?? [],
    selectedNodeId: overrides.selectedNodeId,
    selectedEdgeId: overrides.selectedEdgeId,
    onNodeClick: overrides.onNodeClick,
    onNodeDoubleClick: overrides.onNodeDoubleClick,
    onEdgeClick: overrides.onEdgeClick,
    onBackgroundClick: overrides.onBackgroundClick,
    width: overrides.width ?? 640,
    height: overrides.height ?? 480,
    running: overrides.running ?? false,
  };
  const view = render(<ForceGraph {...props} />);
  const graph = screen.getByRole('img', { name: '关系图谱力导向图' });
  return { ...view, graph, rerenderGraph: (next: Partial<typeof props>) => view.rerender(<ForceGraph {...props} {...next} />) };
}

describe('ForceGraph', () => {
  it('ends panning when the pointer is released outside the graph', () => {
    const { graph } = renderGraph();
    const view = viewport(graph);

    fireEvent(
      graph,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 30,
        clientY: 20,
      }),
    );
    expect(view).toHaveAttribute('transform', 'translate(20 10) scale(1)');

    fireEvent(window, new MouseEvent('pointerup', { bubbles: true }));
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 60,
        clientY: 50,
      }),
    );
    expect(view).toHaveAttribute('transform', 'translate(20 10) scale(1)');
  });

  it('ends panning when the window loses focus', () => {
    const { graph } = renderGraph();
    const view = viewport(graph);

    fireEvent(
      graph,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent.blur(window);
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 50,
        clientY: 30,
      }),
    );

    expect(view).toHaveAttribute('transform', 'translate(0 0) scale(1)');
  });

  it('renders node and edge geometry from deterministic positions', () => {
    const nodes = [makeNode('n1', 'Alpha'), makeNode('n2', 'Beta')];
    const edges = [makeEdge('e1', 'n1', 'n2')];
    const { graph } = renderGraph({ nodes, edges });

    const nodeGroups = Array.from(graph.querySelectorAll('g[data-node]'));
    expect(nodeGroups).toHaveLength(2);
    expect(nodeGroups[0]).toHaveAttribute(
      'transform',
      `translate(${deterministicNodePosition('n1', 640, 480).x} ${deterministicNodePosition('n1', 640, 480).y})`,
    );
    expect(nodeGroups[0].querySelector('text')).toHaveTextContent('Alpha');
    expect(nodeGroups[0].querySelector('title')).toHaveTextContent('Alpha');

    const edgeGroups = Array.from(graph.querySelectorAll('g[data-edge]'));
    expect(edgeGroups).toHaveLength(1);
    const line = edgeGroups[0].querySelector('line')!;
    expect(Number(line.getAttribute('x1'))).toBeCloseTo(
      deterministicNodePosition('n1', 640, 480).x,
    );
    expect(Number(line.getAttribute('x2'))).toBeCloseTo(
      deterministicNodePosition('n2', 640, 480).x,
    );
    expect(line.getAttribute('stroke-width')).toBe('1.5');
  });

  it('falls back to the node id when a label is missing', () => {
    const { graph } = renderGraph({ nodes: [makeNode('node:bare', '')] });

    const group = graph.querySelector('g[data-node]')!;
    expect(group.querySelector('text')).toHaveTextContent('node:bare');
    expect(group.querySelector('title')).toHaveTextContent('node:bare');
  });

  it('selects nodes on pointer down and expands them on double click', () => {
    const node = makeNode('n1', 'Alpha');
    const onNodeClick = vi.fn();
    const onNodeDoubleClick = vi.fn();
    const { graph } = renderGraph({
      nodes: [node, makeNode('n2', 'Beta')],
      onNodeClick,
      onNodeDoubleClick,
    });

    const group = graph.querySelector('g[data-node]')!;
    const circle = group.querySelector('circle')!;
    fireEvent.pointerDown(circle, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
    expect(onNodeClick).toHaveBeenCalledWith(node);
    fireEvent.pointerUp(circle, { pointerId: 1 });

    fireEvent.doubleClick(group);
    expect(onNodeDoubleClick).toHaveBeenCalledWith(node);
  });

  it('selects edges without bubbling into the background handler', () => {
    const edge = makeEdge('e1', 'n1', 'n2');
    const onEdgeClick = vi.fn();
    const onBackgroundClick = vi.fn();
    const { graph } = renderGraph({
      nodes: [makeNode('n1'), makeNode('n2')],
      edges: [edge],
      onEdgeClick,
      onBackgroundClick,
    });

    fireEvent.click(graph.querySelector('g[data-edge] line')!);
    expect(onEdgeClick).toHaveBeenCalledWith(edge);
    expect(onBackgroundClick).not.toHaveBeenCalled();
  });

  it('clears the selection when the background is clicked', () => {
    const onBackgroundClick = vi.fn();
    const { graph } = renderGraph({ onBackgroundClick });

    fireEvent.click(graph);

    expect(onBackgroundClick).toHaveBeenCalledTimes(1);
  });

  it('highlights the selected node and keeps its label on crowded graphs', () => {
    const nodes = Array.from({ length: 41 }, (_, index) =>
      makeNode(`n${index}`, `Node ${index}`),
    );
    const edges = [
      makeEdge('e1', 'n0', 'n1'),
      makeEdge('e2', 'n0', 'n2'),
      makeEdge('e3', 'n0', 'n3'),
    ];
    const { graph } = renderGraph({ nodes, edges, selectedNodeId: 'n40' });

    const groups = Array.from(graph.querySelectorAll('g[data-node]'));
    expect(groups).toHaveLength(41);
    const labeled = groups.filter((group) => group.querySelector('text') !== null);
    expect(labeled).toHaveLength(2);

    const selected = groups[40];
    expect(selected).toHaveAttribute('data-node');
    expect(selected.querySelectorAll('circle')).toHaveLength(2);
    expect(selected.querySelectorAll('circle')[0]).toHaveAttribute('r', '10');
    expect(selected.querySelector('text')).toHaveTextContent('Node 40');
    expect(groups[0].querySelector('text')).toHaveTextContent('Node 0');
  });

  it('hides low-degree labels when the graph exceeds the label budget', () => {
    const nodes = Array.from({ length: 41 }, (_, index) =>
      makeNode(`n${index}`, `Node ${index}`),
    );
    const { graph } = renderGraph({ nodes });

    const groups = Array.from(graph.querySelectorAll('g[data-node]'));
    expect(groups.filter((group) => group.querySelector('text') !== null)).toHaveLength(0);
  });

  it('draws a highlight overlay for the selected edge', () => {
    const { graph } = renderGraph({
      nodes: [makeNode('n1'), makeNode('n2')],
      edges: [makeEdge('e1', 'n1', 'n2')],
      selectedEdgeId: 'e1',
    });

    const lines = graph.querySelectorAll('g[data-edge] line');
    expect(lines).toHaveLength(2);
    expect(lines[0].getAttribute('stroke-width')).toBe('2.5');
    expect(lines[1].getAttribute('stroke-width')).toBe('5');
    expect(lines[1].getAttribute('pointer-events')).toBe('none');
  });

  it('skips node and edge rendering until a viewport is measured', () => {
    const { graph } = renderGraph({
      nodes: [makeNode('n1')],
      edges: [makeEdge('e1', 'n1', 'n2')],
      width: 0,
      height: 0,
    });

    expect(graph.querySelectorAll('g[data-node]')).toHaveLength(0);
    expect(graph.querySelectorAll('g[data-edge]')).toHaveLength(0);
  });

  it('skips edges that reference nodes without positions', () => {
    const { graph } = renderGraph({
      nodes: [makeNode('n1')],
      edges: [makeEdge('e1', 'n1', 'missing')],
    });

    expect(graph.querySelectorAll('g[data-node]')).toHaveLength(1);
    expect(graph.querySelectorAll('g[data-edge]')).toHaveLength(0);
  });

  it('removes simulated nodes that disappear on rerender', () => {
    const { graph, rerenderGraph } = renderGraph({
      nodes: [makeNode('n1', 'Alpha'), makeNode('n2', 'Beta')],
      edges: [makeEdge('e1', 'n1', 'n2')],
    });
    expect(graph.querySelectorAll('g[data-node]')).toHaveLength(2);

    rerenderGraph({ nodes: [makeNode('n1', 'Alpha')], edges: [] });

    const groups = Array.from(graph.querySelectorAll('g[data-node]'));
    expect(groups).toHaveLength(1);
    expect(groups[0].querySelector('text')).toHaveTextContent('Alpha');
    expect(graph.querySelectorAll('g[data-edge]')).toHaveLength(0);
  });

  it('zooms with the canvas controls and clamps the scale', () => {
    const { graph } = renderGraph({ width: 0, height: 0 });
    const view = viewport(graph);

    for (let index = 0; index < 15; index += 1) {
      fireEvent.click(screen.getByRole('button', { name: '放大' }));
    }
    expect(view).toHaveAttribute('transform', 'translate(0 0) scale(5)');

    for (let index = 0; index < 60; index += 1) {
      fireEvent.click(screen.getByRole('button', { name: '缩小' }));
    }
    expect(parseTransform(view).k).toBeCloseTo(0.1);

    fireEvent.click(screen.getByRole('button', { name: '适应视图' }));
    expect(parseTransform(view)).toEqual({ x: 0, y: 0, k: 1 });
  });

  it('zooms around the wheel position', () => {
    const { graph } = renderGraph({ nodes: [makeNode('n1')] });
    const view = viewport(graph);
    vi.spyOn(graph, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 640,
      bottom: 480,
      width: 640,
      height: 480,
      toJSON: () => ({}),
    } as DOMRect);

    const before = parseTransform(view);
    fireEvent.wheel(graph, { deltaY: -100, clientX: 50, clientY: 60 });

    const after = parseTransform(view);
    expect(after.k).toBeCloseTo(Math.min(before.k * 1.1, 5));
    expect(after.k).toBeGreaterThan(before.k);
  });

  it('ignores secondary-button drags and gestures that start on a node label', () => {
    const { graph } = renderGraph({ nodes: [makeNode('n1', 'Alpha')] });
    const view = viewport(graph);
    const initialTransform = view.getAttribute('transform');

    fireEvent(
      graph,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 2,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent(graph, new MouseEvent('pointermove', { bubbles: true, clientX: 40, clientY: 40 }));
    expect(view).toHaveAttribute('transform', initialTransform);

    fireEvent(
      graph.querySelector('g[data-node] text')!,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent(graph, new MouseEvent('pointermove', { bubbles: true, clientX: 90, clientY: 90 }));
    expect(view).toHaveAttribute('transform', initialTransform);
  });

  it('drags a node to world coordinates until the pointer is lifted', () => {
    const node = makeNode('n1', 'Alpha');
    const { graph } = renderGraph({ nodes: [node] });
    const group = graph.querySelector('g[data-node]')!;
    const circle = group.querySelector('circle')!;
    const transform = parseTransform(viewport(graph));

    fireEvent(
      circle,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 120,
        clientY: 90,
      }),
    );

    const worldX = (120 - transform.x) / transform.k;
    const worldY = (90 - transform.y) / transform.k;
    expect(group).toHaveAttribute('transform', `translate(${worldX} ${worldY})`);

    fireEvent(circle, new MouseEvent('pointerup', { bubbles: true }));
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 400,
        clientY: 400,
      }),
    );
    expect(group).toHaveAttribute('transform', `translate(${worldX} ${worldY})`);
  });

  it('restores the deterministic layout through the reset control', () => {
    const { graph } = renderGraph({ nodes: [makeNode('n1', 'Alpha')] });
    const group = graph.querySelector('g[data-node]')!;
    const circle = group.querySelector('circle')!;

    fireEvent(
      circle,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent(graph, new MouseEvent('pointermove', { bubbles: true, clientX: 300, clientY: 250 }));
    expect(group.getAttribute('transform')).not.toBe(
      `translate(${deterministicNodePosition('n1', 640, 480).x} ${deterministicNodePosition('n1', 640, 480).y})`,
    );

    fireEvent.click(screen.getByRole('button', { name: '重置布局' }));

    expect(group).toHaveAttribute(
      'transform',
      `translate(${deterministicNodePosition('n1', 640, 480).x} ${deterministicNodePosition('n1', 640, 480).y})`,
    );
  });

  it('stops panning on pointer cancel and lost pointer capture', () => {
    const { graph } = renderGraph();
    const view = viewport(graph);

    fireEvent(graph, new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
    fireEvent(graph, new MouseEvent('pointercancel', { bubbles: true }));
    fireEvent(graph, new MouseEvent('pointermove', { bubbles: true, clientX: 40, clientY: 40 }));
    expect(view).toHaveAttribute('transform', 'translate(0 0) scale(1)');

    fireEvent(graph, new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
    fireEvent.lostPointerCapture(graph);
    fireEvent(graph, new MouseEvent('pointermove', { bubbles: true, clientX: 40, clientY: 40 }));
    expect(view).toHaveAttribute('transform', 'translate(0 0) scale(1)');
  });

  it('animates the layout while running and stops once it stabilizes', () => {
    const nodes = [makeNode('n1'), makeNode('n2')];
    const edges = [makeEdge('e1', 'n1', 'n2')];
    const { graph, rerenderGraph } = renderGraph({ nodes, edges, running: true });
    const group = graph.querySelector('g[data-node]')!;
    const initial = group.getAttribute('transform');
    expect(frames.size).toBe(1);

    takeFrame();
    expect(group.getAttribute('transform')).not.toBe(initial);

    let iterations = 0;
    while (frames.size > 0 && iterations < 300) {
      takeFrame();
      iterations += 1;
    }
    expect(frames.size).toBe(0);

    rerenderGraph({ running: false });
    expect(frames.size).toBe(0);
  });

  it('does not animate when started paused', () => {
    renderGraph({ nodes: [makeNode('n1')], running: false });

    expect(frames.size).toBe(0);
  });
});
