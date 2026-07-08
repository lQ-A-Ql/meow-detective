import { GitBranch, Pause, Play, RefreshCw } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ForceGraph } from './ForceGraph';
import {
  ALL_EDGE_TYPES,
  buildEdgeMap,
  buildNodeMap,
  edgeTypeColor,
  EDGE_LABELS,
  NODE_LABELS,
} from './graph-utils';
import { SectionHeader } from '@/features/dashboard/components/V3ScoreCards';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { useCurrentCase } from '@/features/case/hooks';
import {
  useGraphQuery,
  useGraphSnapshot,
  useNodeNeighborhood,
  useProvenanceChain,
} from '@/features/graph/hooks';
import { useFileTree } from '@/features/files/hooks';
import type { EdgeType, GraphEdge, GraphNode, GraphProvenanceEntry, NodeType } from '@/types/models';

const MAX_SEEDS = 6;

export function GraphVisualizationSection() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id ?? '';
  const snapshot = useGraphSnapshot(caseId);
  const fileTree = useFileTree();

  const [seedIds, setSeedIds] = useState<string[]>([]);
  const [maxDepth, setMaxDepth] = useState(2);
  const [selectedEdgeTypes, setSelectedEdgeTypes] = useState<EdgeType[]>([...ALL_EDGE_TYPES]);
  const [running, setRunning] = useState(true);
  const [graphData, setGraphData] = useState<{ nodes: GraphNode[]; edges: GraphEdge[] }>({
    nodes: [],
    edges: [],
  });
  const [selectedNodeId, setSelectedNodeId] = useState<string | undefined>();
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | undefined>();
  const [expandTarget, setExpandTarget] = useState<{ nodeId: string; depth: number } | undefined>();

  const canvasRef = useRef<HTMLDivElement>(null);
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    if (!canvasRef.current) return;
    const el = canvasRef.current;
    const ro = new ResizeObserver((entries) => {
      const cr = entries[0].contentRect;
      setCanvasSize({ width: cr.width, height: cr.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    setSeedIds([]);
    setGraphData({ nodes: [], edges: [] });
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }, [caseId]);

  useEffect(() => {
    if (seedIds.length > 0) return;
    if (!fileTree.data || fileTree.data.length === 0) return;
    const ids = fileTree.data.slice(0, MAX_SEEDS).map((n) => n.id);
    setSeedIds(ids);
  }, [fileTree.data, seedIds.length]);

  const initialQuery = useGraphQuery({
    startIds: seedIds,
    edgeTypes: selectedEdgeTypes,
    maxDepth,
    limit: 150,
  });

  useEffect(() => {
    if (initialQuery.data) {
      setGraphData({ nodes: initialQuery.data.nodes, edges: initialQuery.data.edges });
      setSelectedNodeId(undefined);
      setSelectedEdgeId(undefined);
    }
  }, [initialQuery.data]);

  const neighborhood = useNodeNeighborhood(expandTarget?.nodeId ?? '', expandTarget?.depth ?? 1);

  useEffect(() => {
    if (neighborhood.data && expandTarget) {
      mergeGraph(neighborhood.data.nodes, neighborhood.data.edges);
      setExpandTarget(undefined);
    }
  }, [neighborhood.data, expandTarget]);

  const provenance = useProvenanceChain(selectedEdgeId);

  const nodeMap = useMemo(() => buildNodeMap(graphData.nodes), [graphData.nodes]);
  const edgeMap = useMemo(() => buildEdgeMap(graphData.edges), [graphData.edges]);

  const selectedNode = selectedNodeId ? nodeMap.get(selectedNodeId) : undefined;
  const selectedEdge = selectedEdgeId ? edgeMap.get(selectedEdgeId) : undefined;

  function mergeGraph(newNodes: GraphNode[], newEdges: GraphEdge[]) {
    setGraphData((prev) => {
      const nodesMap = buildNodeMap(prev.nodes);
      const edgesMap = buildEdgeMap(prev.edges);
      const nodes = [...prev.nodes];
      const edges = [...prev.edges];
      for (const node of newNodes) {
        if (!nodesMap.has(node.id)) {
          nodesMap.set(node.id, node);
          nodes.push(node);
        }
      }
      for (const edge of newEdges) {
        if (!edgesMap.has(edge.id)) {
          edgesMap.set(edge.id, edge);
          edges.push(edge);
        }
      }
      return { nodes, edges };
    });
  }

  function expandNode(nodeId: string, depth: number) {
    setExpandTarget({ nodeId, depth });
  }

  function toggleEdgeType(type: EdgeType) {
    setSelectedEdgeTypes((prev) =>
      prev.includes(type) ? prev.filter((t) => t !== type) : [...prev, type],
    );
  }

  function selectAllEdgeTypes(selected: boolean) {
    setSelectedEdgeTypes(selected ? [...ALL_EDGE_TYPES] : []);
  }

  async function refresh() {
    await Promise.all([snapshot.refetch(), fileTree.refetch(), initialQuery.refetch()]);
  }

  const hasNodes = graphData.nodes.length > 0;
  const isLoadingGraph = fileTree.isLoading || initialQuery.isLoading;

  return (
    <section>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <SectionHeader icon={GitBranch} title="关系图谱" subtitle="节点、边与邻域展开" />
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-2 text-[11px]">
            <span className="text-forensics-muted">深度</span>
            <Select value={String(maxDepth)} onValueChange={(value) => setMaxDepth(Number(value))}>
              <SelectTrigger size="xs" variant="forensics" className="w-16">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="1">1</SelectItem>
                <SelectItem value="2">2</SelectItem>
                <SelectItem value="3">3</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Button
            type="button"
            variant="outline"
            onClick={() => setRunning((r) => !r)}
            disabled={!hasNodes}
            className="h-7 rounded border-[#ddd] bg-white px-2 text-[11px] hover:bg-[#f5f5f5]"
          >
            {running ? <Pause size={12} className="mr-1" /> : <Play size={12} className="mr-1" />}
            {running ? '暂停' : '继续'}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={refresh}
            disabled={isLoadingGraph}
            className="h-7 rounded border-[#ddd] bg-white px-2 text-[11px] hover:bg-[#f5f5f5]"
          >
            <RefreshCw size={12} className={isLoadingGraph ? 'mr-1 animate-spin' : 'mr-1'} />
            刷新
          </Button>
        </div>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <span className="text-[11px] text-forensics-muted">关系类型:</span>
        <Button
          type="button"
          variant="forensicsGhost"
          size="compact"
          onClick={() => selectAllEdgeTypes(selectedEdgeTypes.length < ALL_EDGE_TYPES.length)}
          className="text-[10px]"
        >
          {selectedEdgeTypes.length < ALL_EDGE_TYPES.length ? '全选' : '清空'}
        </Button>
        {ALL_EDGE_TYPES.map((type) => (
          <label
            key={type}
            className="flex cursor-pointer items-center gap-1 rounded border border-[#e8e8e8] bg-white px-2 py-1 text-[10px] hover:bg-[#f5f5f5]"
          >
            <Checkbox
              checked={selectedEdgeTypes.includes(type)}
              onCheckedChange={() => toggleEdgeType(type)}
              variant="forensics"
              checkboxSize="compact"
            />
            <span
              className="inline-block h-2 w-2 rounded-full"
              style={{ backgroundColor: edgeTypeColor(type) }}
            />
            <span>{EDGE_LABELS[type]}</span>
          </label>
        ))}
      </div>

      <div className="mt-3 flex h-[420px] overflow-hidden rounded border border-[#e0e0e0] bg-[#fafafa]">
        <div ref={canvasRef} className="relative flex-1">
          {!hasNodes && !isLoadingGraph ? (
            <div className="flex h-full flex-col items-center justify-center p-6 text-center text-[12px] text-forensics-muted">
              <div>暂无图数据可可视化。</div>
              <div className="mt-1">导入数据源并运行分析提取后，将在此处渲染关系网络。</div>
            </div>
          ) : (
            <ForceGraph
              nodes={graphData.nodes}
              edges={graphData.edges}
              selectedNodeId={selectedNodeId}
              selectedEdgeId={selectedEdgeId}
              onNodeClick={(node) => {
                setSelectedNodeId(node.id);
                setSelectedEdgeId(undefined);
              }}
              onNodeDoubleClick={(node) => expandNode(node.id, 1)}
              onEdgeClick={(edge) => {
                setSelectedEdgeId(edge.id);
                setSelectedNodeId(undefined);
              }}
              onBackgroundClick={() => {
                setSelectedNodeId(undefined);
                setSelectedEdgeId(undefined);
              }}
              width={canvasSize.width}
              height={canvasSize.height}
              running={running}
            />
          )}
          {isLoadingGraph ? (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-white/60">
              <div className="flex items-center gap-2 rounded border border-[#e0e0e0] bg-white px-3 py-2 text-[11px] shadow-sm">
                <RefreshCw size={12} className="animate-spin" />
                加载图数据...
              </div>
            </div>
          ) : null}
        </div>

        <div className="w-64 shrink-0 overflow-auto border-l border-[#e0e0e0] bg-white p-3">
          {selectedNode ? (
            <NodeMiniDetails
              node={selectedNode}
              edges={graphData.edges}
              onExpand={(depth) => expandNode(selectedNode.id, depth)}
              onClose={() => setSelectedNodeId(undefined)}
            />
          ) : selectedEdge ? (
            <EdgeMiniDetails
              edge={selectedEdge}
              nodeMap={nodeMap}
              provenance={provenance.data}
              provenanceLoading={provenance.isLoading}
              onClose={() => setSelectedEdgeId(undefined)}
            />
          ) : (
            <div className="space-y-3 text-[11px] text-forensics-muted">
              <p>单击节点查看详情，双击节点展开邻域；单击边查看来源追溯。</p>
              <p>拖拽节点调整位置，拖拽空白处平移，滚轮缩放。</p>
              {snapshot.data ? (
                <div className="space-y-1 rounded border border-[#e8e8e8] bg-white p-2">
                  <div className="flex justify-between">
                    <span>节点</span>
                    <span className="font-mono text-forensics-text">{snapshot.data.totalNodes}</span>
                  </div>
                  <div className="flex justify-between">
                    <span>边</span>
                    <span className="font-mono text-forensics-text">{snapshot.data.totalEdges}</span>
                  </div>
                  <div className="flex justify-between">
                    <span>密度</span>
                    <span className="font-mono text-forensics-text">{snapshot.data.density}</span>
                  </div>
                </div>
              ) : null}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function NodeMiniDetails({
  node,
  edges,
  onExpand,
  onClose,
}: {
  node: GraphNode;
  edges: GraphEdge[];
  onExpand: (depth: number) => void;
  onClose: () => void;
}) {
  const degree = edges.filter((e) => e.sourceId === node.id || e.targetId === node.id).length;
  return (
    <div className="space-y-3 text-[11px]">
      <div className="font-medium text-forensics-text">{node.label || '(无标签)'}</div>
      <div className="grid grid-cols-2 gap-2">
        <div className="rounded border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">类型</div>
          <div className="font-medium text-forensics-text">
            {NODE_LABELS[node.nodeType as NodeType] ?? node.nodeType}
          </div>
        </div>
        <div className="rounded border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">度</div>
          <div className="font-medium text-forensics-text">{degree}</div>
        </div>
      </div>
      <div className="break-all rounded border border-forensics-border bg-forensics-surface p-2 font-mono text-[10px] text-forensics-text-secondary">
        {node.id}
      </div>
      <div className="break-words rounded border border-forensics-border bg-forensics-surface p-2 text-forensics-text-secondary">
        {node.summary || '(无摘要)'}
      </div>
      <div className="flex flex-wrap gap-1.5">
        <Button type="button" size="sm" variant="outline" onClick={() => onExpand(1)} className="h-6 text-[10px]">
          展开 1 层
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={() => onExpand(2)} className="h-6 text-[10px]">
          展开 2 层
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onClose} className="h-6 text-[10px]">
          关闭
        </Button>
      </div>
    </div>
  );
}

function EdgeMiniDetails({
  edge,
  nodeMap,
  provenance,
  provenanceLoading,
  onClose,
}: {
  edge: GraphEdge;
  nodeMap: Map<string, GraphNode>;
  provenance?: GraphProvenanceEntry[];
  provenanceLoading: boolean;
  onClose: () => void;
}) {
  const source = nodeMap.get(edge.sourceId);
  const target = nodeMap.get(edge.targetId);
  return (
    <div className="space-y-3 text-[11px]">
      <div className="font-medium text-forensics-text">{EDGE_LABELS[edge.edgeType]}</div>
      <div className="grid grid-cols-2 gap-2">
        <div className="rounded border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">置信度</div>
          <div className="font-medium text-forensics-text">
            {edge.confidence !== undefined ? edge.confidence : '-'}
          </div>
        </div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">源</div>
        <div className="break-all rounded border border-forensics-border bg-forensics-surface p-1.5 font-mono text-[10px]">
          {source?.label ?? edge.sourceId}
        </div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">目标</div>
        <div className="break-all rounded border border-forensics-border bg-forensics-surface p-1.5 font-mono text-[10px]">
          {target?.label ?? edge.targetId}
        </div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">来源追溯</div>
        {provenanceLoading ? (
          <div className="text-forensics-muted">加载中...</div>
        ) : provenance && provenance.length > 0 ? (
          <div className="space-y-1">
            {provenance.map((entry, i) => (
              <div key={`${entry.edgeId}-${i}`} className="rounded border border-forensics-border bg-forensics-surface p-1.5">
                {entry.sourceParser ? <div>解析器: {entry.sourceParser}</div> : null}
                {entry.sourceRuleId ? <div>规则: {entry.sourceRuleId}</div> : null}
                {entry.parserVersion ? <div>版本: {entry.parserVersion}</div> : null}
              </div>
            ))}
          </div>
        ) : (
          <div className="text-forensics-muted">无追溯记录</div>
        )}
      </div>
      <Button type="button" size="sm" variant="ghost" onClick={onClose} className="h-6 text-[10px]">
        关闭
      </Button>
    </div>
  );
}
