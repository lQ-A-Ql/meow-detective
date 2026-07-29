import { useEffect, useMemo, useState } from 'react';
import { GitBranch, Link2, ListFilter, Network, Search, TimerReset } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { ToggleGroup, ToggleGroupItem } from '@/app/components/ui/toggle-group';
import type {
  CorrelationConfidence,
  CorrelationSnapshot,
} from '@/types/models';
import { isHighConfidenceLead, isReviewLead, OverviewCard } from './correlation-helpers';
import { CorrelationFamilyCoveragePanel, LeadCard } from './LeadList';
import { LeadDetailPanel } from './LeadDetail';
import { ClusterCard } from './ClusterView';

export function CorrelationWorkspace({
  snapshot,
  onRefresh,
  refreshing = false,
  onJumpToTarget,
}: {
  snapshot: CorrelationSnapshot;
  onRefresh?: () => void;
  refreshing?: boolean;
  onJumpToTarget?: (route: string, targetId: string) => void;
}) {
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

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Network size={16} />
          关联分析工作台
        </div>
        {onRefresh ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={onRefresh}
            className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"
          >
            <TimerReset size={14} className={refreshing ? 'opacity-70' : ''} />
            刷新关联
          </Button>
        ) : null}
      </div>

      <div className="grid grid-cols-2 gap-3 xl:grid-cols-6">
        <OverviewCard label="Lead" value={snapshot.leadCount.toString()} />
        <OverviewCard label="Cluster" value={snapshot.clusterCount.toString()} />
        <OverviewCard label="Node" value={snapshot.nodeCount.toString()} />
        <OverviewCard label="Edge" value={snapshot.edgeCount.toString()} />
        <OverviewCard label="规则家族" value={snapshot.familyCoverage.length.toString()} />
        <OverviewCard label="已覆盖家族" value={coveredFamilyCount.toString()} />
      </div>

      <CorrelationFamilyCoveragePanel items={snapshot.familyCoverage} />

      <div className="flex flex-col gap-3 rounded-none border border-forensics-border bg-forensics-surface p-4 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex flex-1 flex-col gap-3 xl:flex-row xl:items-center">
          <label className="relative block w-full xl:max-w-[320px]">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-forensics-muted-light" />
            <Input
              value={leadSearch}
              onChange={(event) => setLeadSearch(event.target.value)}
              placeholder="搜索线索标题、路径、信号"
              className="pl-9"
              data-testid="correlation-lead-search"
            />
          </label>

          <ToggleGroup
            type="single"
            value={focusMode}
            onValueChange={(value) => {
              if (value === 'all' || value === 'highConfidence' || value === 'review') {
                setFocusMode(value);
              }
            }}
            variant="outline"
            size="sm"
            className="w-full xl:w-auto"
          >
            <ToggleGroupItem value="all" aria-label="显示全部线索">
              全部
            </ToggleGroupItem>
            <ToggleGroupItem value="highConfidence" aria-label="只看高置信线索">
              高置信
            </ToggleGroupItem>
            <ToggleGroupItem value="review" aria-label="只看待复核线索">
              待复核
            </ToggleGroupItem>
          </ToggleGroup>

          <Select
            value={confidenceFilter}
            onValueChange={(value) => setConfidenceFilter(value as 'all' | CorrelationConfidence)}
          >
            <SelectTrigger className="w-full xl:w-[180px]" data-testid="correlation-confidence-filter">
              <SelectValue placeholder="全部置信度" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全部置信度</SelectItem>
              <SelectItem value="direct">Direct</SelectItem>
              <SelectItem value="strong">Strong</SelectItem>
              <SelectItem value="weak">Weak</SelectItem>
              <SelectItem value="heuristic">Heuristic</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-wrap items-center gap-2 text-[11px] text-forensics-text-tertiary">
          <div className="flex items-center gap-2 rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
            <ListFilter size={14} className="text-forensics-muted-light" />
            <span>
              显示 {filteredLeads.length} / {snapshot.leadCount} 条线索
            </span>
          </div>
          <div className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
            高置信 {highConfidenceLeadCount}
          </div>
          <div className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
            待复核 {reviewLeadCount}
          </div>
          <div className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
            生成时间 {snapshot.generatedAt.slice(0, 19).replace('T', ' ')}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <div className="space-y-4">
          <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
              <GitBranch size={14} />
              线索总览
            </div>
            <div className="space-y-3">
              {filteredLeads.length > 0 ? (
                filteredLeads.map((lead) => (
                  <LeadCard
                    key={lead.id}
                    lead={lead}
                    selected={lead.id === selectedLead?.id}
                    onJump={jumpToTarget}
                    onSelect={() => setSelectedLeadId(lead.id)}
                  />
                ))
              ) : (
                <div className="text-[12px] text-forensics-muted">当前筛选条件下没有可展示的关联线索。</div>
              )}
            </div>
          </div>

          <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
              <Link2 size={14} />
              证据聚合
            </div>
            <div className="space-y-3">
              {filteredClusters.length > 0 ? (
                filteredClusters.map((cluster) => (
                  <ClusterCard key={cluster.id} cluster={cluster} onJump={jumpToTarget} />
                ))
              ) : (
                <div className="text-[12px] text-forensics-muted">当前筛选条件下没有可展示的聚合 cluster。</div>
              )}
            </div>
          </div>
        </div>

        <div className="space-y-4">
          {selectedLead ? (
            <LeadDetailPanel
              lead={selectedLead}
              primaryFileNode={primaryFileNode}
              supportingNodes={selectedSupportingNodes}
              edges={selectedLeadEdges}
              relatedClusters={relatedClusters}
              onJump={jumpToTarget}
            />
          ) : (
            <div className="rounded-none border border-forensics-border bg-forensics-surface p-4 text-[12px] text-forensics-muted">
              当前暂无可展开的 lead 明细。
            </div>
          )}

          <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-3 text-[12px] font-light text-forensics-text">节点分布</div>
            <div className="space-y-2 text-[11px] text-forensics-text-tertiary">
              {distributionNodes.map((node) => (
                <div key={node.id} className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <div className="truncate font-light text-forensics-text">{node.title}</div>
                    <span className="font-mono text-[10px] uppercase text-forensics-muted-light">{node.kind}</span>
                  </div>
                  {node.subtitle ? <div className="mt-1 break-all text-forensics-muted">{node.subtitle}</div> : null}
                </div>
              ))}
            </div>
          </div>

          <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-3 text-[12px] font-light text-forensics-text">当前规则边界</div>
            <ul className="space-y-2 text-[11px] text-forensics-text-tertiary">
              <li>当前已接入 source object、路径匹配、名称匹配与 Recycle Bin 原路径恢复。</li>
              <li>线索用于 investigator 导航，不输出结论型判定。</li>
              <li>名称类与时间线类命中仍需回跳原始事件或工件字段复核。</li>
            </ul>
          </div>
        </div>
      </div>
    </section>
  );
}
