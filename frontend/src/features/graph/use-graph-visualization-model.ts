import { useEffect, useMemo, useState } from 'react';
import { useCurrentCase } from '@/features/case/hooks';
import {
  useGraphQuery,
  useGraphSnapshot,
  useNodeNeighborhood,
  useProvenanceChain,
} from '@/features/graph/hooks';
import { ALL_EDGE_TYPES, buildEdgeMap, buildNodeMap } from '@/features/graph/components/graph-utils';
import type { EdgeType, GraphEdge, GraphNode } from '@/types/models';

const MAX_SEEDS = 6;

export function useGraphVisualizationModel() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id ?? '';
  const snapshot = useGraphSnapshot(caseId);
  const [seedIds, setSeedIds] = useState<string[]>([]);
  const [maxDepth, setMaxDepth] = useState(2);
  const [selectedEdgeTypes, setSelectedEdgeTypes] = useState<EdgeType[]>([...ALL_EDGE_TYPES]);
  const [running, setRunning] = useState(true);
  const [graphData, setGraphData] = useState<{ nodes: GraphNode[]; edges: GraphEdge[] }>({
    nodes: [],
    edges: [],
  });
  const [selectedNodeId, setSelectedNodeId] = useState<string>();
  const [selectedEdgeId, setSelectedEdgeId] = useState<string>();
  const [expandTarget, setExpandTarget] = useState<{ nodeId: string; depth: number }>();
  const hasSelectedEdgeTypes = selectedEdgeTypes.length > 0;
  const initialQuery = useGraphQuery({
    startIds: hasSelectedEdgeTypes ? seedIds : [],
    edgeTypes: selectedEdgeTypes,
    maxDepth,
    limit: 150,
    edgeLimit: 600,
  });
  const neighborhood = useNodeNeighborhood(expandTarget?.nodeId ?? '', expandTarget?.depth ?? 1);
  const provenance = useProvenanceChain(selectedEdgeId);

  useEffect(() => {
    setSeedIds([]);
    setGraphData({ nodes: [], edges: [] });
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }, [caseId]);

  useEffect(() => {
    setSeedIds(snapshot.data?.seedIds.slice(0, MAX_SEEDS) ?? []);
  }, [snapshot.data?.seedIds]);

  useEffect(() => {
    if (hasSelectedEdgeTypes) return;
    setGraphData({ nodes: [], edges: [] });
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }, [hasSelectedEdgeTypes]);

  useEffect(() => {
    if (!initialQuery.data) return;
    setGraphData({ nodes: initialQuery.data.nodes, edges: initialQuery.data.edges });
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }, [initialQuery.data]);

  useEffect(() => {
    if (!neighborhood.data || !expandTarget) return;
    setGraphData((previous) => mergeGraphData(previous, neighborhood.data.nodes, neighborhood.data.edges));
    setExpandTarget(undefined);
  }, [neighborhood.data, expandTarget]);

  const nodeMap = useMemo(() => buildNodeMap(graphData.nodes), [graphData.nodes]);
  const edgeMap = useMemo(() => buildEdgeMap(graphData.edges), [graphData.edges]);
  const selectedNode = selectedNodeId ? nodeMap.get(selectedNodeId) : undefined;
  const selectedEdge = selectedEdgeId ? edgeMap.get(selectedEdgeId) : undefined;

  return {
    graphData,
    nodeMap,
    selectedNode,
    selectedEdge,
    selectedNodeId,
    selectedEdgeId,
    selectedEdgeTypes,
    maxDepth,
    running,
    snapshot: snapshot.data,
    provenance: provenance.data,
    provenanceLoading: provenance.isLoading,
    truncated: initialQuery.data?.truncated ?? false,
    hasNodes: graphData.nodes.length > 0,
    isLoadingGraph: snapshot.isLoading || (hasSelectedEdgeTypes && initialQuery.isLoading),
    setMaxDepth,
    toggleRunning: () => setRunning((value) => !value),
    toggleEdgeType(type: EdgeType) {
      setSelectedEdgeTypes((previous) =>
        previous.includes(type) ? previous.filter((item) => item !== type) : [...previous, type],
      );
    },
    selectAllEdgeTypes(selected: boolean) {
      setSelectedEdgeTypes(selected ? [...ALL_EDGE_TYPES] : []);
    },
    selectNode(nodeId?: string) {
      setSelectedNodeId(nodeId);
      if (nodeId) setSelectedEdgeId(undefined);
    },
    selectEdge(edgeId?: string) {
      setSelectedEdgeId(edgeId);
      if (edgeId) setSelectedNodeId(undefined);
    },
    clearSelection() {
      setSelectedNodeId(undefined);
      setSelectedEdgeId(undefined);
    },
    expandNode(nodeId: string, depth: number) {
      setExpandTarget({ nodeId, depth });
    },
    async refresh() {
      await snapshot.refetch();
      if (hasSelectedEdgeTypes && seedIds.length > 0) await initialQuery.refetch();
    },
  };
}

function mergeGraphData(
  previous: { nodes: GraphNode[]; edges: GraphEdge[] },
  newNodes: GraphNode[],
  newEdges: GraphEdge[],
) {
  const nodeMap = buildNodeMap(previous.nodes);
  const edgeMap = buildEdgeMap(previous.edges);
  const nodes = [...previous.nodes];
  const edges = [...previous.edges];
  for (const node of newNodes) {
    if (!nodeMap.has(node.id)) {
      nodeMap.set(node.id, node);
      nodes.push(node);
    }
  }
  for (const edge of newEdges) {
    if (!edgeMap.has(edge.id)) {
      edgeMap.set(edge.id, edge);
      edges.push(edge);
    }
  }
  return { nodes, edges };
}

export type GraphVisualizationModel = ReturnType<typeof useGraphVisualizationModel>;
