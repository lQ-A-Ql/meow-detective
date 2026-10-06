import { useEffect, useMemo, useState } from 'react';
import { useCurrentCase } from '@/features/case/hooks';
import {
  useGraphQuery,
  useGraphSnapshot,
  useNodeNeighborhood,
  useProvenanceChain,
} from '@/features/graph/hooks';
import { ALL_EDGE_TYPES, buildEdgeMap, buildNodeMap } from '@/features/graph/logic/graph-utils';
import { MAX_GRAPH_EDGES, MAX_GRAPH_NODES, mergeGraphData, type GraphData } from '@/features/graph/logic/graph-data';
import type { EdgeType } from '@/types/models';

const MAX_SEEDS = 6;

export function useGraphVisualizationModel() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id ?? '';
  const snapshot = useGraphSnapshot(caseId);
  const [seedIds, setSeedIds] = useState<string[]>([]);
  const [maxDepth, setMaxDepth] = useState(2);
  const [selectedEdgeTypes, setSelectedEdgeTypes] = useState<EdgeType[]>([...ALL_EDGE_TYPES]);
  const [running, setRunning] = useState(true);
  const [graphData, setGraphData] = useState<GraphData>({
    nodes: [],
    edges: [],
    truncated: false,
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
    setGraphData({ nodes: [], edges: [], truncated: false });
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }, [caseId]);

  useEffect(() => {
    setSeedIds(snapshot.data?.seedIds.slice(0, MAX_SEEDS) ?? []);
  }, [snapshot.data?.seedIds]);

  useEffect(() => {
    if (hasSelectedEdgeTypes) return;
    setGraphData({ nodes: [], edges: [], truncated: false });
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }, [hasSelectedEdgeTypes]);

  useEffect(() => {
    if (!initialQuery.data) return;
    setGraphData({
      nodes: initialQuery.data.nodes.slice(0, MAX_GRAPH_NODES),
      edges: initialQuery.data.edges.slice(0, MAX_GRAPH_EDGES),
      truncated: initialQuery.data.truncated
        || initialQuery.data.nodes.length > MAX_GRAPH_NODES
        || initialQuery.data.edges.length > MAX_GRAPH_EDGES,
    });
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
    truncated: graphData.truncated,
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
      if (graphData.nodes.length >= MAX_GRAPH_NODES || graphData.edges.length >= MAX_GRAPH_EDGES) return;
      setExpandTarget({ nodeId, depth });
    },
    async refresh() {
      await snapshot.refetch();
      if (hasSelectedEdgeTypes && seedIds.length > 0) await initialQuery.refetch();
    },
  };
}

export type GraphVisualizationModel = ReturnType<typeof useGraphVisualizationModel>;
