import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { RefObject } from 'react';
import { GraphVisualizationSectionView } from '@/features/graph/components/GraphVisualizationSectionView';
import { ALL_EDGE_TYPES } from '@/features/graph/logic/graph-utils';
import type { GraphVisualizationModel } from '@/features/graph/use-graph-visualization-model';
import type { GraphEdge, GraphNode, GraphSnapshot } from '@/types/models';

function makeNode(id: string, label: string): GraphNode {
  return {
    id,
    caseId: 'case-1',
    nodeType: 'file',
    label,
    summary: `Summary for ${label}`,
    tags: [],
    createdAt: '2026-06-14T00:00:00Z',
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

function makeModel(overrides: Partial<GraphVisualizationModel> = {}): GraphVisualizationModel {
  return {
    graphData: { nodes: [], edges: [], truncated: false },
    nodeMap: new Map(),
    selectedNode: undefined,
    selectedEdge: undefined,
    selectedNodeId: undefined,
    selectedEdgeId: undefined,
    selectedEdgeTypes: [...ALL_EDGE_TYPES],
    maxDepth: 2,
    running: false,
    snapshot: undefined,
    provenance: undefined,
    provenanceLoading: false,
    truncated: false,
    hasNodes: false,
    isLoadingGraph: false,
    setMaxDepth: vi.fn(),
    toggleRunning: vi.fn(),
    toggleEdgeType: vi.fn(),
    selectAllEdgeTypes: vi.fn(),
    selectNode: vi.fn(),
    selectEdge: vi.fn(),
    clearSelection: vi.fn(),
    expandNode: vi.fn(),
    refresh: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as GraphVisualizationModel;
}

const canvasRef: RefObject<HTMLDivElement> = { current: null };
const canvasSize = { width: 640, height: 480 };

function renderView(model: GraphVisualizationModel) {
  return render(
    <GraphVisualizationSectionView model={model} canvasRef={canvasRef} canvasSize={canvasSize} />,
  );
}

describe('GraphVisualizationSectionView', () => {
  it('renders the empty state when there are no nodes', () => {
    const model = makeModel();
    renderView(model);

    expect(screen.getByText('暂无确定性的跨数据源实体关联。')).toBeInTheDocument();
    expect(screen.getByText('完成至少两个数据源的痕迹与实体提取后，将在此处显示案件关系网络。')).toBeInTheDocument();
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.getByRole('button', { name: '暂停' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '刷新' })).toBeEnabled();
  });

  it('renders the force graph and wires selection callbacks', () => {
    const nodes = [makeNode('n1', 'Alpha'), makeNode('n2', 'Beta')];
    const edge = makeEdge('e1', 'n1', 'n2');
    const model = makeModel({
      graphData: { nodes, edges: [edge], truncated: false },
      hasNodes: true,
    });
    renderView(model);

    const graph = screen.getByRole('img', { name: '关系图谱力导向图' });
    fireEvent.pointerDown(graph.querySelector('g[data-node] circle')!, {
      button: 0,
      clientX: 10,
      clientY: 10,
    });
    expect(model.selectNode).toHaveBeenCalledWith('n1');
    fireEvent.click(graph.querySelector('g[data-edge] line')!);
    expect(model.selectEdge).toHaveBeenCalledWith('e1');

    fireEvent.doubleClick(graph.querySelectorAll('g[data-node]')[1]);
    expect(model.expandNode).toHaveBeenCalledWith('n2', 1);

    fireEvent.click(graph);
    expect(model.clearSelection).toHaveBeenCalled();
  });

  it('shows the loading overlay without the empty state', () => {
    const model = makeModel({ isLoadingGraph: true, hasNodes: false });
    renderView(model);

    expect(screen.getByText('加载图数据...')).toBeInTheDocument();
    expect(screen.queryByText('暂无确定性的跨数据源实体关联。')).toBeNull();
    expect(screen.getByRole('button', { name: '刷新' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '暂停' })).toBeDisabled();
  });

  it('toggles simulation and refreshes the graph', async () => {
    const model = makeModel({ hasNodes: true });
    renderView(model);

    fireEvent.click(screen.getByRole('button', { name: '暂停' }));
    expect(model.toggleRunning).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: '刷新' }));
    await waitFor(() => expect(model.refresh).toHaveBeenCalledTimes(1));
  });

  it('selects all relation types when some are hidden and clears them when all are visible', () => {
    const model = makeModel({ hasNodes: true, selectedEdgeTypes: ['contains', 'references'] });
    const { rerender } = renderView(model);

    fireEvent.click(screen.getByRole('button', { name: '全选 / 清空' }));
    expect(model.selectAllEdgeTypes).toHaveBeenCalledWith(true);

    const allSelected = makeModel({ hasNodes: true, selectedEdgeTypes: [...ALL_EDGE_TYPES] });
    rerender(
      <GraphVisualizationSectionView model={allSelected} canvasRef={canvasRef} canvasSize={canvasSize} />,
    );
    fireEvent.click(screen.getByRole('button', { name: '全选 / 清空' }));
    expect(allSelected.selectAllEdgeTypes).toHaveBeenCalledWith(false);
  });

  it('toggles individual relation type checkboxes', () => {
    const model = makeModel({ hasNodes: true, selectedEdgeTypes: ['contains'] });
    renderView(model);

    expect(screen.getAllByRole('checkbox')).toHaveLength(ALL_EDGE_TYPES.length);
    fireEvent.click(screen.getByRole('checkbox', { name: '包含' }));

    expect(model.toggleEdgeType).toHaveBeenCalledWith('contains');
  });

  it('updates the depth through the selector', () => {
    const model = makeModel({ hasNodes: true, maxDepth: 2 });
    renderView(model);

    fireEvent.click(screen.getByRole('combobox'));
    fireEvent.click(screen.getByRole('option', { name: '3' }));

    expect(model.setMaxDepth).toHaveBeenCalledWith(3);
  });

  it('renders node details with expand and close actions', () => {
    const node = makeNode('n1', 'Alpha');
    const model = makeModel({
      graphData: { nodes: [node], edges: [], truncated: false },
      nodeMap: new Map([[node.id, node]]),
      selectedNode: node,
      selectedNodeId: node.id,
      hasNodes: true,
    });
    renderView(model);

    expect(screen.getByText(node.summary)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '展开 1 层' }));
    fireEvent.click(screen.getByRole('button', { name: '展开 2 层' }));
    expect(model.expandNode).toHaveBeenNthCalledWith(1, 'n1', 1);
    expect(model.expandNode).toHaveBeenNthCalledWith(2, 'n1', 2);

    fireEvent.click(screen.getByRole('button', { name: '关闭' }));
    expect(model.selectNode).toHaveBeenCalledWith();
  });

  it('renders edge details with provenance and close action', () => {
    const edge = makeEdge('e1', 'n1', 'n2');
    const source = makeNode('n1', 'Source artifact');
    const target = makeNode('n2', 'Target event');
    const model = makeModel({
      graphData: { nodes: [], edges: [edge], truncated: false },
      nodeMap: new Map([
        [source.id, source],
        [target.id, target],
      ]),
      selectedEdge: edge,
      selectedEdgeId: edge.id,
      hasNodes: true,
      provenance: [{ edgeId: edge.id, sourceParser: 'evtx' }],
    });
    renderView(model);

    expect(screen.getByText('Source artifact')).toBeInTheDocument();
    expect(screen.getByText('Target event')).toBeInTheDocument();
    expect(screen.getByText('解析器: evtx')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '关闭' }));
    expect(model.selectEdge).toHaveBeenCalledWith();
  });

  it('renders summary statistics and the truncation warning', () => {
    const snapshot: GraphSnapshot = {
      nodeCountByType: {},
      edgeCountByType: {},
      totalNodes: 12,
      totalEdges: 20,
      density: 0.3,
      largestComponentSize: 5,
      dataSourceCount: 2,
      crossSourceEntityCount: 3,
      crossSourceEdgeCount: 4,
      seedIds: [],
    };
    const model = makeModel({ snapshot, truncated: true });
    renderView(model);

    expect(screen.getByText('单击节点查看详情，双击节点展开邻域；单击边查看来源追溯。')).toBeInTheDocument();
    expect(screen.getByText('数据源').parentElement).toHaveTextContent('2');
    expect(screen.getByText('跨源实体').parentElement).toHaveTextContent('3');
    expect(screen.getByText('跨源关系').parentElement).toHaveTextContent('4');
    expect(screen.getByText('节点').parentElement).toHaveTextContent('12');
    expect(screen.getByText('边').parentElement).toHaveTextContent('20');
    expect(screen.getByText('密度').parentElement).toHaveTextContent('0.3');
    expect(
      screen.getByText('当前图窗口已达到节点或关系预算，请缩小深度或关系类型。'),
    ).toBeInTheDocument();
  });

  it('omits snapshot statistics and the truncation warning when absent', () => {
    const model = makeModel();
    renderView(model);

    expect(screen.queryByText('数据源')).toBeNull();
    expect(
      screen.queryByText('当前图窗口已达到节点或关系预算，请缩小深度或关系类型。'),
    ).toBeNull();
  });
});
