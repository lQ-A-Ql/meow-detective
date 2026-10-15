import { useTranslation } from 'react-i18next';
import { useEffect, useMemo, useState } from 'react';
import type { CorrelationConfidence, CorrelationSnapshot } from '@/types/models';
import { isHighConfidenceLead, isReviewLead } from './correlation-lead-classifiers';

export function useCorrelationWorkspaceModel({
  snapshot: rawSnapshot,
  onRefresh,
  refreshing = false,
  onJumpToTarget,
}: {
  snapshot: CorrelationSnapshot;
  onRefresh?: () => void;
  refreshing?: boolean;
  onJumpToTarget?: (route: string, targetId: string) => void;
}) {

  const { t } = useTranslation();

  // A partial payload (older backend, preview harness) must not blank the whole analysis route:
  // the rest of this view indexes straight into these collections.
  const snapshot: CorrelationSnapshot = {
    ...rawSnapshot,
    leads: rawSnapshot.leads ?? [],
    nodes: rawSnapshot.nodes ?? [],
    edges: rawSnapshot.edges ?? [],
    clusters: rawSnapshot.clusters ?? [],
    familyCoverage: rawSnapshot.familyCoverage ?? [],
    leadCount: rawSnapshot.leadCount ?? 0,
    clusterCount: rawSnapshot.clusterCount ?? 0,
    nodeCount: rawSnapshot.nodeCount ?? 0,
    edgeCount: rawSnapshot.edgeCount ?? 0,
    generatedAt: rawSnapshot.generatedAt ?? '',
  };

  const [selectedLeadId, setSelectedLeadId] = useState<string | undefined>(snapshot.leads[0]?.id);

  const [leadSearch, setLeadSearch] = useState('');

  const [focusMode, setFocusMode] = useState<'all' | 'highConfidence' | 'review'>('all');

  const [confidenceFilter, setConfidenceFilter] = useState<'all' | CorrelationConfidence>('all');


  const nodeMap = useMemo(
    () => new Map(snapshot.nodes.map((node) => [node.id, node])),
    [snapshot.nodes],
  );


  const filteredLeads = useMemo(() => {
    const normalizedSearch = leadSearch.trim().toLowerCase();
    return snapshot.leads.filter((lead) => {
      if (focusMode === 'highConfidence' && !isHighConfidenceLead(lead)) {
        return false;
      }
      if (focusMode === 'review' && !isReviewLead(lead)) {
        return false;
      }
      if (confidenceFilter !== 'all' && lead.confidence !== confidenceFilter) {
        return false;
      }
      if (!normalizedSearch) {
        return true;
      }

      const primaryNode = snapshot.nodes.find((node) => {
        if (node.kind !== 'file') {
          return false;
        }
        return (
          node.sourceObjectId === lead.primaryFileId
          || node.id === lead.primaryFileId
          || node.id === `file:${lead.primaryFileId}`
        );
      });
      const supportingNodes = lead.supportingNodeIds
        .map((id) => nodeMap.get(id))
        .filter((node): node is NonNullable<typeof node> => Boolean(node));
      const searchableParts = [
        lead.title,
        lead.summary,
        lead.primaryFileId,
        ...lead.matchSignals,
        ...lead.caveats,
        ...lead.provenance.map((item) => item.sourceLabel),
        primaryNode?.title,
        primaryNode?.subtitle,
        ...supportingNodes.flatMap((node) => [node.title, node.subtitle]),
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase();

      return searchableParts.includes(normalizedSearch);
    });
  }, [confidenceFilter, focusMode, leadSearch, nodeMap, snapshot.leads, snapshot.nodes]);


  useEffect(() => {
    if (filteredLeads.length === 0) {
      setSelectedLeadId(undefined);
      return;
    }
    if (!selectedLeadId || !filteredLeads.some((lead) => lead.id === selectedLeadId)) {
      setSelectedLeadId(filteredLeads[0].id);
    }
  }, [filteredLeads, selectedLeadId]);


  const jumpToTarget = onJumpToTarget ?? (() => undefined);


  const selectedLead = useMemo(
    () => filteredLeads.find((lead) => lead.id === selectedLeadId) ?? filteredLeads[0],
    [filteredLeads, selectedLeadId],
  );


  const primaryFileNode = useMemo(() => {
    if (!selectedLead) {
      return undefined;
    }
    return snapshot.nodes.find((node) => {
      if (node.kind !== 'file') {
        return false;
      }
      return (
        node.sourceObjectId === selectedLead.primaryFileId
        || node.id === selectedLead.primaryFileId
        || node.id === `file:${selectedLead.primaryFileId}`
      );
    });
  }, [selectedLead, snapshot.nodes]);


  const selectedSupportingNodes = useMemo(() => {
    if (!selectedLead) {
      return [];
    }
    const supportingIds = new Set(selectedLead.supportingNodeIds);
    return snapshot.nodes.filter((node) => supportingIds.has(node.id));
  }, [selectedLead, snapshot.nodes]);


  const selectedLeadEdges = useMemo(() => {
    if (!selectedLead) {
      return [];
    }
    const relatedNodeIds = new Set(selectedLead.supportingNodeIds);
    if (primaryFileNode) {
      relatedNodeIds.add(primaryFileNode.id);
    }
    return snapshot.edges.filter(
      (edge) => relatedNodeIds.has(edge.fromNodeId) || relatedNodeIds.has(edge.toNodeId),
    );
  }, [primaryFileNode, selectedLead, snapshot.edges]);


  const relatedClusters = useMemo(() => {
    if (!selectedLead) {
      return [];
    }
    return snapshot.clusters.filter((cluster) => cluster.primaryFileId === selectedLead.primaryFileId);
  }, [selectedLead, snapshot.clusters]);


  const filteredClusters = useMemo(() => {
    const primaryFileIds = new Set(filteredLeads.map((lead) => lead.primaryFileId));
    return snapshot.clusters.filter((cluster) => primaryFileIds.has(cluster.primaryFileId));
  }, [filteredLeads, snapshot.clusters]);


  const visibleNodeIds = useMemo(() => {
    const ids = new Set<string>();
    for (const lead of filteredLeads.slice(0, 6)) {
      ids.add(`file:${lead.primaryFileId}`);
      ids.add(lead.primaryFileId);
      for (const nodeId of lead.supportingNodeIds) {
        ids.add(nodeId);
      }
    }
    return ids;
  }, [filteredLeads]);


  const distributionNodes = useMemo(() => {
    const focusedNodes = snapshot.nodes.filter((node) => {
      if (visibleNodeIds.has(node.id)) {
        return true;
      }
      return Boolean(node.sourceObjectId && visibleNodeIds.has(node.sourceObjectId));
    });
    if (focusedNodes.length > 0) {
      return focusedNodes.slice(0, 8);
    }
    return snapshot.nodes.slice(0, 8);
  }, [snapshot.nodes, visibleNodeIds]);


  const reviewLeadCount = useMemo(
    () => snapshot.leads.filter((lead) => isReviewLead(lead)).length,
    [snapshot.leads],
  );

  const highConfidenceLeadCount = useMemo(
    () => snapshot.leads.filter((lead) => isHighConfidenceLead(lead)).length,
    [snapshot.leads],
  );

  const coveredFamilyCount = useMemo(
    () => snapshot.familyCoverage.filter((item) => item.status === 'covered').length,
    [snapshot.familyCoverage],
  );
return { onRefresh, refreshing, t, snapshot, setSelectedLeadId, leadSearch, setLeadSearch, focusMode, setFocusMode, confidenceFilter, setConfidenceFilter, filteredLeads, jumpToTarget, selectedLead, primaryFileNode, selectedSupportingNodes, selectedLeadEdges, relatedClusters, filteredClusters, distributionNodes, reviewLeadCount, highConfidenceLeadCount, coveredFamilyCount };
}
