import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GraphEdge, GraphNode } from '@/types/models';
import { deterministicNodePosition, fitTransform } from '@/features/graph/logic/graph-utils';
import { useForceGraphModel } from '@/features/graph/use-force-graph-model';

type HookProps = Parameters<typeof useForceGraphModel>[0];

function makeNode(id: string, overrides: Partial<GraphNode> = {}): GraphNode {
  return {
    id,
    caseId: 'case-1',
    nodeType: 'file',
    label: id,
    summary: '',
    tags: [],
    createdAt: '2026-06-14T00:00:00Z',
    ...overrides,
  };
}

function makeEdge(id: string, sourceId: string, targetId: string): GraphEdge {
  return {
    id,
    caseId: 'case-1',
    sourceId,
    targetId,
    edgeType: 'references',
    createdAt: '2026-06-14T00:00:00Z',
  };
}

function makeSvg(rect: Partial<DOMRect> = {}) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  vi.spyOn(svg, 'getBoundingClientRect').mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 800,
    bottom: 600,
    width: 800,
    height: 600,
    toJSON: () => ({}),
    ...rect,
  } as DOMRect);
  return svg;
}

function pointerEvent<T extends Element = SVGSVGElement>(overrides: {
  button?: number;
  clientX?: number;
  clientY?: number;
  pointerId?: number;
  target?: Element;
  currentTarget?: Element;
} = {}) {
  return {
    button: 0,
    clientX: 0,
    clientY: 0,
    pointerId: 1,
    target: document.createElement('div'),
    currentTarget: makeSvg(),
    stopPropagation: vi.fn(),
    preventDefault: vi.fn(),
    ...overrides,
  } as unknown as React.PointerEvent<T>;
}

const nodes: GraphNode[] = [makeNode('a'), makeNode('b'), makeNode('c')];
const edges: GraphEdge[] = [makeEdge('e1', 'a', 'b'), makeEdge('e2', 'a', 'c')];

let frames: Map<number, FrameRequestCallback>;
let nextFrameId: number;

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

function baseProps(overrides: Partial<HookProps> = {}): HookProps {
  return {
    nodes: [],
    edges: [],
    width: 800,
    height: 600,
    running: false,
    ...overrides,
  };
}

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

describe('useForceGraphModel', () => {
  it('starts with an identity transform and empty simulation state', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ width: 0, height: 0 })),
    );

    expect(result.current.positions.size).toBe(0);
    expect(result.current.transform).toEqual({ x: 0, y: 0, k: 1 });
    expect(result.current.degrees.size).toBe(0);
    expect(result.current.initialFitRef.current).toBe(false);
    expect(result.current.simulationTickRef.current).toBe(0);
    expect(result.current.showAllLabels).toBe(true);
    expect(result.current.selectedNodeId).toBeUndefined();
    expect(result.current.selectedEdgeId).toBeUndefined();
  });

  it('initializes deterministic positions sized by node degree', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges })),
    );

    expect(result.current.degrees.get('a')).toBe(2);
    expect(result.current.degrees.get('b')).toBe(1);
    expect(result.current.degrees.get('c')).toBe(1);
    expect(result.current.positions.size).toBe(3);

    const nodeA = result.current.positions.get('a')!;
    expect(nodeA.x).toBe(deterministicNodePosition('a', 800, 600).x);
    expect(nodeA.y).toBe(deterministicNodePosition('a', 800, 600).y);
    expect(nodeA.radius).toBeCloseTo(6 + Math.sqrt(2) * 2);
    expect(result.current.positions.get('b')!.radius).toBeCloseTo(8);
    expect(result.current.initialFitRef.current).toBe(true);
    expect(result.current.transform).toEqual(
      fitTransform(result.current.positions, 800, 600),
    );
  });

  it('waits for a non-zero viewport before placing nodes', () => {
    const { result, rerender } = renderHook(
      (props: HookProps) => useForceGraphModel(props),
      { initialProps: baseProps({ nodes, edges, width: 0 }) },
    );

    expect(result.current.positions.size).toBe(0);
    expect(result.current.transform).toEqual({ x: 0, y: 0, k: 1 });

    rerender(baseProps({ nodes, edges }));

    expect(result.current.positions.size).toBe(3);
    expect(result.current.initialFitRef.current).toBe(true);
  });

  it('drops simulated entries for nodes that leave the graph', () => {
    const { result, rerender } = renderHook(
      (props: HookProps) => useForceGraphModel(props),
      { initialProps: baseProps({ nodes, edges }) },
    );
    expect(result.current.positions.size).toBe(3);

    rerender(baseProps({ nodes: [makeNode('a')], edges: [] }));

    expect(Array.from(result.current.positions.keys())).toEqual(['a']);
  });

  it('resets simulation counters when the node signature changes', () => {
    const { result, rerender } = renderHook(
      (props: HookProps) => useForceGraphModel(props),
      { initialProps: baseProps({ nodes, edges }) },
    );

    act(() => {
      result.current.simulationTickRef.current = 9;
      result.current.stableTickRef.current = 4;
    });

    rerender(baseProps({ nodes: [makeNode('a'), makeNode('b')], edges }));

    expect(result.current.simulationTickRef.current).toBe(0);
    expect(result.current.stableTickRef.current).toBe(0);
  });

  it('labels every node while the graph stays under the label budget', () => {
    const many = Array.from({ length: 40 }, (_, index) => makeNode(`n${index}`));
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes: many, width: 0, height: 0 })),
    );
    expect(result.current.showAllLabels).toBe(true);

    const extra = [...many, makeNode('n40')];
    const { result: crowded } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes: extra, width: 0, height: 0 })),
    );
    expect(crowded.current.showAllLabels).toBe(false);
  });

  it('clamps zoom controls between the minimum and maximum scale', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ width: 0, height: 0 })),
    );

    act(() => {
      for (let index = 0; index < 20; index += 1) result.current.zoomIn();
    });
    expect(result.current.transform.k).toBe(5);

    act(() => {
      for (let index = 0; index < 60; index += 1) result.current.zoomOut();
    });
    expect(result.current.transform.k).toBe(0.1);
  });

  it('fits the view to the current node bounds', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges })),
    );

    act(() => {
      result.current.setPositions((previous) => {
        const next = new Map(previous);
        const node = next.get('a')!;
        node.x = 5000;
        node.y = -400;
        return next;
      });
    });

    act(() => {
      result.current.fit();
    });

    expect(result.current.transform).toEqual(
      fitTransform(result.current.positions, 800, 600),
    );
  });

  it('resets nodes to their deterministic layout and clears tick counters', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges })),
    );

    act(() => {
      result.current.setPositions((previous) => {
        const next = new Map(previous);
        const node = next.get('a')!;
        node.x = 4321;
        node.y = 1234;
        return next;
      });
      result.current.simulationTickRef.current = 30;
      result.current.stableTickRef.current = 10;
    });

    act(() => {
      result.current.reset();
    });

    const nodeA = result.current.positions.get('a')!;
    expect(nodeA.x).toBe(deterministicNodePosition('a', 800, 600).x);
    expect(nodeA.y).toBe(deterministicNodePosition('a', 800, 600).y);
    expect(nodeA.vx).toBe(0);
    expect(nodeA.vy).toBe(0);
    expect(result.current.simulationTickRef.current).toBe(0);
    expect(result.current.stableTickRef.current).toBe(0);
  });

  it('pans the viewport with primary-button pointer drags', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ width: 0, height: 0 })),
    );

    act(() => {
      result.current.handlePointerDown(pointerEvent({ clientX: 10, clientY: 10 }));
    });
    expect(result.current.isPanningRef.current).toBe(true);

    act(() => {
      result.current.handlePointerMove(pointerEvent({ clientX: 40, clientY: 30 }));
    });

    expect(result.current.transform).toEqual({ x: 30, y: 20, k: 1 });

    act(() => {
      result.current.handlePointerUp(pointerEvent());
    });
    expect(result.current.isPanningRef.current).toBe(false);
  });

  it('ignores non-primary buttons and gestures that start on a node', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ width: 0, height: 0 })),
    );

    act(() => {
      result.current.handlePointerDown(pointerEvent({ button: 2 }));
    });
    expect(result.current.isPanningRef.current).toBe(false);

    const nodeTarget = document.createElement('div');
    nodeTarget.setAttribute('data-node', '');
    act(() => {
      result.current.handlePointerDown(pointerEvent({ target: nodeTarget }));
    });
    expect(result.current.isPanningRef.current).toBe(false);

    act(() => {
      result.current.handlePointerMove(pointerEvent({ clientX: 100, clientY: 100 }));
    });
    expect(result.current.transform).toEqual({ x: 0, y: 0, k: 1 });
  });

  it('drags a node to pointer coordinates converted into world space', () => {
    const onNodeClick = vi.fn();
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges, onNodeClick })),
    );
    const svg = makeSvg();
    Object.defineProperty(result.current.containerRef, 'current', {
      configurable: true,
      value: svg,
    });

    act(() => {
      result.current.handleNodePointerDown(
        pointerEvent<SVGCircleElement>({ currentTarget: svg, clientX: 10, clientY: 10 }),
        nodes[0],
      );
    });

    expect(onNodeClick).toHaveBeenCalledWith(nodes[0]);
    expect(result.current.dragNodeIdRef.current).toBe('a');
    expect(result.current.simulationTickRef.current).toBe(0);

    act(() => {
      result.current.handlePointerMove(pointerEvent({ clientX: 120, clientY: 90 }));
    });

    const transform = result.current.transform;
    const node = result.current.positions.get('a')!;
    expect(node.x).toBeCloseTo((120 - transform.x) / transform.k);
    expect(node.y).toBeCloseTo((90 - transform.y) / transform.k);
    expect(node.vx).toBe(0);
    expect(node.vy).toBe(0);

    act(() => {
      result.current.handleNodePointerUp(
        pointerEvent<SVGCircleElement>({ currentTarget: svg }),
      );
    });
    expect(result.current.dragNodeIdRef.current).toBeNull();
  });

  it('supports node drags without a click callback and without a canvas rect', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges })),
    );

    act(() => {
      result.current.handleNodePointerDown(pointerEvent<SVGCircleElement>(), nodes[0]);
    });
    expect(result.current.dragNodeIdRef.current).toBe('a');

    act(() => {
      result.current.handlePointerMove(pointerEvent({ clientX: 50, clientY: 50 }));
    });

    const node = result.current.positions.get('a')!;
    expect(node.x).toBe(deterministicNodePosition('a', 800, 600).x);
    expect(node.y).toBe(deterministicNodePosition('a', 800, 600).y);
  });

  it('clears drag and pan state when the window loses the pointer', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges })),
    );

    act(() => {
      result.current.handleNodePointerDown(pointerEvent<SVGCircleElement>(), nodes[0]);
      result.current.handlePointerDown(pointerEvent({ clientX: 5, clientY: 5 }));
    });
    expect(result.current.dragNodeIdRef.current).toBe('a');
    expect(result.current.isPanningRef.current).toBe(true);

    act(() => {
      window.dispatchEvent(new Event('pointerup'));
    });
    expect(result.current.dragNodeIdRef.current).toBeNull();
    expect(result.current.isPanningRef.current).toBe(false);

    act(() => {
      result.current.handlePointerDown(pointerEvent({ clientX: 5, clientY: 5 }));
      window.dispatchEvent(new Event('pointercancel'));
    });
    expect(result.current.isPanningRef.current).toBe(false);

    act(() => {
      result.current.handlePointerDown(pointerEvent({ clientX: 5, clientY: 5 }));
      window.dispatchEvent(new Event('blur'));
    });
    expect(result.current.isPanningRef.current).toBe(false);
  });

  it('zooms toward the wheel position around the canvas origin', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ width: 0, height: 0 })),
    );
    Object.defineProperty(result.current.containerRef, 'current', {
      configurable: true,
      value: makeSvg({ left: 10, top: 20 }),
    });

    const zoomInEvent = pointerEvent({ clientX: 110, clientY: 120 }) as unknown as React.WheelEvent<SVGSVGElement>;
    (zoomInEvent as unknown as { deltaY: number }).deltaY = -100;

    act(() => {
      result.current.handleWheel(zoomInEvent);
    });

    expect(zoomInEvent.preventDefault).toHaveBeenCalled();
    expect(result.current.transform.k).toBeCloseTo(1.1);
    expect(result.current.transform.x).toBeCloseTo(-10);
    expect(result.current.transform.y).toBeCloseTo(-10);

    const beforeZoomOut = result.current.transform.k;
    const zoomOutEvent = pointerEvent({ clientX: 110, clientY: 120 }) as unknown as React.WheelEvent<SVGSVGElement>;
    (zoomOutEvent as unknown as { deltaY: number }).deltaY = 100;

    act(() => {
      result.current.handleWheel(zoomOutEvent);
    });

    expect(result.current.transform.k).toBeLessThan(beforeZoomOut);
  });

  it('stops wheel zooming when no canvas is mounted', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ width: 0, height: 0 })),
    );
    const wheelEvent = pointerEvent() as unknown as React.WheelEvent<SVGSVGElement>;
    (wheelEvent as unknown as { deltaY: number }).deltaY = -100;

    act(() => {
      result.current.handleWheel(wheelEvent);
    });

    expect(result.current.transform).toEqual({ x: 0, y: 0, k: 1 });
  });

  it('advances the simulation one frame at a time while running', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges, running: true })),
    );
    expect(frames.size).toBe(1);
    const beforeX = result.current.positions.get('a')!.x;

    takeFrame();

    expect(result.current.simulationTickRef.current).toBe(1);
    expect(result.current.positions.get('a')!.x).not.toBe(beforeX);
    expect(frames.size).toBe(1);
  });

  it('does not schedule simulation frames when paused', () => {
    renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges, running: false })),
    );

    expect(frames.size).toBe(0);
  });

  it('stops the loop once the stable-tick target is reached', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges, running: true })),
    );

    act(() => {
      result.current.stableTickRef.current = 12;
    });

    takeFrame();

    expect(result.current.simulationTickRef.current).toBe(0);
    expect(frames.size).toBe(0);
  });

  it('stops the loop at the maximum simulation tick budget', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges, running: true })),
    );

    act(() => {
      result.current.simulationTickRef.current = 239;
    });

    takeFrame();
    expect(result.current.simulationTickRef.current).toBe(240);
    expect(frames.size).toBe(1);

    takeFrame();
    expect(result.current.simulationTickRef.current).toBe(240);
    expect(frames.size).toBe(0);
  });

  it('clears pointer interaction state directly', () => {
    const { result } = renderHook(() =>
      useForceGraphModel(baseProps({ nodes, edges })),
    );

    act(() => {
      result.current.handleNodePointerDown(pointerEvent<SVGCircleElement>(), nodes[0]);
    });
    expect(result.current.dragNodeIdRef.current).toBe('a');

    act(() => {
      result.current.clearPointerInteraction();
    });

    expect(result.current.dragNodeIdRef.current).toBeNull();
    expect(result.current.isPanningRef.current).toBe(false);
  });
});
