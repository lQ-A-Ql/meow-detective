import { useEffect, useMemo, useState } from 'react';
import { GitBranch, Link2, ListFilter, Network, Search, TimerReset } from 'lucide-react';
import { useNavigate } from 'react-router';
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
import { useSelectionStore } from '@/stores/selection-store';
import type {
  CorrelationCluster,
  CorrelationConfidence,
  CorrelationCoverageStatus,
  CorrelationFamilyCoverage,
  CorrelationLead,
  CorrelationSnapshot,
} from '@/types/models';

function confidenceLabel(value: CorrelationConfidence) {
  switch (value) {
    case 'direct':
      return 'Direct';
    case 'strong':
      return 'Strong';
    case 'weak':
      return 'Weak';
    case 'heuristic':
      return 'Heuristic';
    default:
      return value;
  }
}

function confidenceTone(value: CorrelationConfidence) {
  switch (value) {
    case 'direct':
      return 'border-[#0d7a32] bg-[#effaf2] text-[#0d7a32]';
    case 'strong':
      return 'border-[#175cd3] bg-[#eff6ff] text-[#175cd3]';
    case 'weak':
      return 'border-[#b54708] bg-[#fff7ed] text-[#b54708]';
    case 'heuristic':
      return 'border-[#667085] bg-[#f8fafc] text-[#475467]';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

function coverageTone(value: CorrelationCoverageStatus) {
  switch (value) {
    case 'covered':
      return 'border-[#0d7a32] bg-[#effaf2] text-[#0d7a32]';
    case 'review':
      return 'border-[#b54708] bg-[#fff7ed] text-[#b54708]';
    case 'missing':
      return 'border-[#667085] bg-[#f8fafc] text-[#475467]';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

function coverageLabel(value: CorrelationCoverageStatus) {
  switch (value) {
    case 'covered':
      return 'Covered';
    case 'review':
      return 'Review';
    case 'missing':
      return 'Missing';
    default:
      return value;
  }
}

function translateGuarantee(value: string) {
  switch (value) {
    case 'guaranteed':
      return 'Guaranteed';
    case 'bestEffort':
      return 'BestEffort';
    case 'experimental':
      return 'Experimental';
    case 'notGuaranteed':
      return 'NotGuaranteed';
    default:
      return value;
  }
}

function summarizeLeadKinds(lead: CorrelationLead) {
  if (lead.families.length > 0) {
    return lead.families.join(' / ');
  }
  const labels = lead.provenance.map((item) => item.sourceLabel);
  const uniqueLabels = Array.from(new Set(labels));
  if (uniqueLabels.length === 0) {
    return 'RuleMatch';
  }
  return uniqueLabels.join(' / ');
}

function isReviewLead(lead: CorrelationLead) {
  return (
    lead.caveats.length > 0
    || lead.confidence === 'weak'
    || lead.confidence === 'heuristic'
    || lead.provenance.some(
      (item) => item.guaranteeLevel === 'experimental' || item.guaranteeLevel === 'notGuaranteed',
    )
  );
}

function isHighConfidenceLead(lead: CorrelationLead) {
  return lead.confidence === 'direct' || lead.confidence === 'strong';
}

export function CorrelationWorkspace({
  snapshot,
  onRefresh,
  refreshing = false,
}: {
  snapshot: CorrelationSnapshot;
  onRefresh?: () => void;
  refreshing?: boolean;
}) {
  const navigate = useNavigate();
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const setSelectedArtifactId = useSelectionStore((state) => state.setSelectedArtifactId);
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);
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

  function jumpToTarget(route: string, targetId: string) {
    if (route === '/files') {
      setSelectedFileId(targetId);
      navigate(route);
      return;
    }
    if (route === '/artifacts') {
      setSelectedArtifactId(targetId);
      navigate(route);
      return;
    }
    if (route === '/timeline') {
      setSelectedTimelineId(targetId);
      navigate(route);
      return;
    }
    navigate(route);
  }

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
        <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Network size={16} />
          关联分析工作台
        </div>
        {onRefresh ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={onRefresh}
            className="h-8 rounded border-[#ddd] bg-white px-3 text-[12px] hover:bg-[#f5f5f5]"
          >
            <TimerReset size={14} className={refreshing ? 'animate-spin' : ''} />
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

      <div className="flex flex-col gap-3 rounded border border-[#e0e0e0] bg-white p-4 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex flex-1 flex-col gap-3 xl:flex-row xl:items-center">
          <label className="relative block w-full xl:max-w-[320px]">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-[#888]" />
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

        <div className="flex flex-wrap items-center gap-2 text-[11px] text-[#555]">
          <div className="flex items-center gap-2 rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2">
            <ListFilter size={14} className="text-[#888]" />
            <span>
              显示 {filteredLeads.length} / {snapshot.leadCount} 条线索
            </span>
          </div>
          <div className="rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2">
            高置信 {highConfidenceLeadCount}
          </div>
          <div className="rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2">
            待复核 {reviewLeadCount}
          </div>
          <div className="rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2">
            生成时间 {snapshot.generatedAt.slice(0, 19).replace('T', ' ')}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[0.95fr_1.05fr]">
        <div className="space-y-4">
          <div className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-semibold text-[#111]">
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
                <div className="text-[12px] text-[#666]">当前筛选条件下没有可展示的关联线索。</div>
              )}
            </div>
          </div>

          <div className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-semibold text-[#111]">
              <Link2 size={14} />
              证据聚合
            </div>
            <div className="space-y-3">
              {filteredClusters.length > 0 ? (
                filteredClusters.map((cluster) => (
                  <ClusterCard key={cluster.id} cluster={cluster} onJump={jumpToTarget} />
                ))
              ) : (
                <div className="text-[12px] text-[#666]">当前筛选条件下没有可展示的聚合 cluster。</div>
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
            <div className="rounded border border-[#e0e0e0] bg-white p-4 text-[12px] text-[#666]">
              当前暂无可展开的 lead 明细。
            </div>
          )}

          <div className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="mb-3 text-[12px] font-semibold text-[#111]">节点分布</div>
            <div className="space-y-2 text-[11px] text-[#555]">
              {distributionNodes.map((node) => (
                <div key={node.id} className="rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <div className="truncate font-medium text-[#111]">{node.title}</div>
                    <span className="font-mono text-[10px] uppercase text-[#888]">{node.kind}</span>
                  </div>
                  {node.subtitle ? <div className="mt-1 break-all text-[#666]">{node.subtitle}</div> : null}
                </div>
              ))}
            </div>
          </div>

          <div className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="mb-3 text-[12px] font-semibold text-[#111]">当前规则边界</div>
            <ul className="space-y-2 text-[11px] text-[#555]">
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

function CorrelationFamilyCoveragePanel({ items }: { items: CorrelationFamilyCoverage[] }) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white p-4" data-testid="correlation-family-coverage-panel">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <div className="text-[12px] font-semibold text-[#111]">规则家族覆盖</div>
          <div className="mt-1 text-[11px] text-[#666]">
            直接展示关联快照产出的家族覆盖、线索强度与示例信号。
          </div>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-3 2xl:grid-cols-2">
        {items.map((item) => (
          <div
            key={item.family}
            className="rounded border border-[#e5e7eb] bg-[#fcfcfc] p-3"
            data-testid={`correlation-family-${item.family}`}
          >
            <div className="flex items-center justify-between gap-2">
              <div>
                <div className="text-[12px] font-medium text-[#111]">{item.displayName}</div>
                <div className="mt-1 font-mono text-[10px] text-[#888]">{item.family}</div>
              </div>
              <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${coverageTone(item.status)}`}>
                {coverageLabel(item.status)}
              </span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 lg:grid-cols-4">
              <Metric label="Lead" value={item.leadCount.toString()} />
              <Metric label="高置信" value={item.highConfidenceLeadCount.toString()} />
              <Metric label="待复核" value={item.reviewLeadCount.toString()} />
              <Metric label="Cluster" value={item.clusterCount.toString()} />
            </div>
            {item.sampleSignals.length > 0 ? (
              <div className="mt-3 rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
                <div className="mb-1 text-[10px] uppercase tracking-wider text-[#888]">Sample Signals</div>
                <div className="space-y-1">
                  {item.sampleSignals.map((signal) => (
                    <div key={`${item.family}-${signal}`}>{signal}</div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="mt-3 text-[11px] text-[#777]">当前没有可展示的该家族命中信号。</div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function LeadCard({
  lead,
  selected,
  onJump,
  onSelect,
}: {
  lead: CorrelationLead;
  selected: boolean;
  onJump: (route: string, targetId: string) => void;
  onSelect: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      data-testid={`lead-card-${lead.id}`}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      className={`rounded border p-4 transition-colors ${
        selected
          ? 'border-[#111] bg-[#f7f7f7] shadow-[inset_0_0_0_1px_rgba(17,17,17,0.08)]'
          : 'border-[#e0e0e0] bg-[#fcfcfc] hover:border-[#cfcfcf]'
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[13px] font-semibold text-[#111]">{lead.title}</div>
          <div className="mt-1 text-[11px] text-[#555]">{lead.summary}</div>
        </div>
        <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(lead.confidence)}`}>
          {confidenceLabel(lead.confidence)}
        </span>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="主文件" value={lead.primaryFileId} mono />
        <Metric label="支撑节点" value={lead.supportingNodeIds.length.toString()} />
        <Metric label="来源类别" value={summarizeLeadKinds(lead) || '-'} />
        <Metric label="告警数" value={lead.caveats.length.toString()} />
      </div>
      <FamilyPills families={lead.families} testId={`lead-families-${lead.id}`} />
      {lead.matchSignals.length > 0 ? (
        <div className="mt-3 rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
          <div className="mb-1 text-[10px] uppercase tracking-wider text-[#888]">Match Signals</div>
          <div className="space-y-1">
            {lead.matchSignals.map((item) => (
              <div key={`${lead.id}-${item}`}>{item}</div>
            ))}
          </div>
        </div>
      ) : null}
      <div className="mt-3 flex flex-wrap gap-2">
        {lead.provenance.map((item) => (
          <span
            key={`${lead.id}-${item.sourceKind}-${item.sourceRecordId}`}
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555]"
          >
            {item.sourceLabel}
          </span>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {lead.jumps.map((jump) => (
          <button
            key={`${lead.id}-${jump.route}-${jump.targetId}`}
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onJump(jump.route, jump.targetId);
            }}
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555] hover:border-[#bbb] hover:bg-[#f7f7f7] hover:text-[#111]"
          >
            {jump.label}
          </button>
        ))}
      </div>
      {lead.provenance.length > 0 ? (
        <div className="mt-3 space-y-2 text-[11px] text-[#555]">
          {lead.provenance.slice(0, 3).map((item) => (
            <div
              key={`${lead.id}-${item.sourceKind}-${item.sourceRecordId}`}
              className="rounded border border-[#eee] bg-white px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium text-[#111]">{item.sourceLabel}</span>
                <span className="font-mono text-[10px] text-[#888]">{translateGuarantee(item.guaranteeLevel)}</span>
              </div>
              <div className="mt-1 break-all text-[#666]">
                {item.sourceKind} · {item.sourceRecordId}
                {item.producer ? ` · ${item.producer}` : ''}
              </div>
            </div>
          ))}
        </div>
      ) : null}
      {lead.caveats.length > 0 ? (
        <div className="mt-3 rounded border border-amber-200 bg-amber-50 p-3 text-[11px] text-amber-900">
          {lead.caveats.map((item) => (
            <div key={`${lead.id}-${item}`}>{item}</div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function LeadDetailPanel({
  lead,
  primaryFileNode,
  supportingNodes,
  edges,
  relatedClusters,
  onJump,
}: {
  lead: CorrelationLead;
  primaryFileNode?: CorrelationSnapshot['nodes'][number];
  supportingNodes: CorrelationSnapshot['nodes'];
  edges: CorrelationSnapshot['edges'];
  relatedClusters: CorrelationCluster[];
  onJump: (route: string, targetId: string) => void;
}) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white p-4" data-testid="selected-lead-panel">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[12px] uppercase tracking-wider text-[#888]">Lead 明细</div>
          <div className="mt-1 text-[15px] font-semibold text-[#111]" data-testid="selected-lead-title">
            {lead.title}
          </div>
          <div className="mt-2 text-[11px] leading-5 text-[#555]">{lead.summary}</div>
        </div>
        <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(lead.confidence)}`}>
          {confidenceLabel(lead.confidence)}
        </span>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="主文件" value={lead.primaryFileId} mono />
        <Metric label="支撑节点" value={lead.supportingNodeIds.length.toString()} />
        <Metric label="匹配信号" value={lead.matchSignals.length.toString()} />
        <Metric label="Provenance" value={lead.provenance.length.toString()} />
      </div>

      <FamilyPills families={lead.families} testId={`selected-lead-families-${lead.id}`} />

      <div className="mt-4 flex flex-wrap gap-2">
        {lead.jumps.map((jump) => (
          <button
            key={`${lead.id}-detail-${jump.route}-${jump.targetId}`}
            type="button"
            onClick={() => onJump(jump.route, jump.targetId)}
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555] hover:border-[#bbb] hover:bg-[#f7f7f7] hover:text-[#111]"
          >
            {jump.label}
          </button>
        ))}
      </div>

      {lead.matchSignals.length > 0 ? (
        <div className="mt-4 rounded border border-[#e5e7eb] bg-[#fcfcfc] px-3 py-3 text-[11px] text-[#555]">
          <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">Match Signals</div>
          <div className="space-y-1">
            {lead.matchSignals.map((item) => (
              <div key={`${lead.id}-detail-signal-${item}`}>{item}</div>
            ))}
          </div>
        </div>
      ) : null}

      <div className="mt-4 grid grid-cols-1 gap-4 2xl:grid-cols-[1.05fr_0.95fr]">
        <div className="space-y-4">
          <div className="rounded border border-[#eee] bg-[#fcfcfc] p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">关联节点</div>
            <div className="space-y-2">
              {primaryFileNode ? (
                <NodeSummaryCard
                  node={primaryFileNode}
                  title="主文件节点"
                  onJump={onJump}
                />
              ) : null}
              {supportingNodes.length > 0 ? (
                supportingNodes.map((node) => (
                  <NodeSummaryCard
                    key={node.id}
                    node={node}
                    title="支撑节点"
                    onJump={onJump}
                  />
                ))
              ) : (
                <div className="text-[11px] text-[#666]">当前没有额外的支撑节点。</div>
              )}
            </div>
          </div>

          <div className="rounded border border-[#eee] bg-[#fcfcfc] p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">相关边</div>
            {edges.length > 0 ? (
              <div className="space-y-2">
                {edges.map((edge) => (
                  <div key={edge.id} className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-mono text-[10px] text-[#888]">{edge.kind}</span>
                      <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(edge.confidence)}`}>
                        {confidenceLabel(edge.confidence)}
                      </span>
                    </div>
                    <div className="mt-1 text-[#111]">{edge.summary}</div>
                    <div className="mt-1 break-all text-[10px] text-[#777]">
                      {edge.fromNodeId} {'->'} {edge.toNodeId}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-[11px] text-[#666]">当前 lead 尚未挂接可展示的关联边。</div>
            )}
          </div>
        </div>

        <div className="space-y-4">
          <div className="rounded border border-[#eee] bg-[#fcfcfc] p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">相关聚合</div>
            {relatedClusters.length > 0 ? (
              <div className="space-y-2">
                {relatedClusters.map((cluster) => (
                  <div key={cluster.id} className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-[#111]">{cluster.title}</span>
                      <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(cluster.confidence)}`}>
                        {confidenceLabel(cluster.confidence)}
                      </span>
                    </div>
                    <div className="mt-1">{cluster.summary}</div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-[11px] text-[#666]">当前没有同主文件的聚合 cluster。</div>
            )}
          </div>

          <div className="rounded border border-[#eee] bg-[#fcfcfc] p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">Provenance</div>
            {lead.provenance.length > 0 ? (
              <div className="space-y-2">
                {lead.provenance.map((item) => (
                  <div
                    key={`${lead.id}-detail-provenance-${item.sourceKind}-${item.sourceRecordId}`}
                    className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-[#111]">{item.sourceLabel}</span>
                      <span className="font-mono text-[10px] text-[#888]">{translateGuarantee(item.guaranteeLevel)}</span>
                    </div>
                    <div className="mt-1 break-all">
                      {item.sourceKind} · {item.sourceRecordId}
                      {item.producer ? ` · ${item.producer}` : ''}
                    </div>
                    {item.warningSummary.length > 0 ? (
                      <div className="mt-2 rounded border border-amber-200 bg-amber-50 px-2 py-1 text-[10px] text-amber-900">
                        {item.warningSummary.join('；')}
                      </div>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-[11px] text-[#666]">当前没有可展示的 provenance。</div>
            )}
          </div>
        </div>
      </div>

      {lead.caveats.length > 0 ? (
        <div className="mt-4 rounded border border-amber-200 bg-amber-50 p-3 text-[11px] text-amber-900">
          {lead.caveats.map((item) => (
            <div key={`${lead.id}-detail-caveat-${item}`}>{item}</div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function NodeSummaryCard({
  node,
  title,
  onJump,
}: {
  node: CorrelationSnapshot['nodes'][number];
  title: string;
  onJump: (route: string, targetId: string) => void;
}) {
  return (
    <div className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[10px] uppercase tracking-wider text-[#888]">{title}</span>
        <span className="font-mono text-[10px] text-[#888]">{node.kind}</span>
      </div>
      <div className="mt-1 font-medium text-[#111]">{node.title}</div>
      {node.subtitle ? <div className="mt-1 break-all">{node.subtitle}</div> : null}
      {node.badges.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {node.badges.map((badge) => (
            <span key={`${node.id}-${badge}`} className="rounded border border-[#ddd] bg-[#fcfcfc] px-2 py-0.5 text-[10px] text-[#666]">
              {badge}
            </span>
          ))}
        </div>
      ) : null}
      {node.jumps.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {node.jumps.map((jump) => (
            <button
              key={`${node.id}-${jump.route}-${jump.targetId}`}
              type="button"
              onClick={() => onJump(jump.route, jump.targetId)}
              className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555] hover:border-[#bbb] hover:bg-[#f7f7f7] hover:text-[#111]"
            >
              {jump.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function ClusterCard({
  cluster,
  onJump,
}: {
  cluster: CorrelationCluster;
  onJump: (route: string, targetId: string) => void;
}) {
  const primaryJump =
    cluster.primaryFileId
      ? { route: '/files', targetId: cluster.primaryFileId, label: '查看主文件' }
      : undefined;

  return (
    <div className="rounded border border-[#e0e0e0] bg-[#fcfcfc] p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[13px] font-semibold text-[#111]">{cluster.title}</div>
          <div className="mt-1 text-[11px] text-[#555]">{cluster.summary}</div>
        </div>
        <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(cluster.confidence)}`}>
          {confidenceLabel(cluster.confidence)}
        </span>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="Artifact" value={cluster.artifactCount.toString()} />
        <Metric label="Timeline" value={cluster.timelineCount.toString()} />
        <Metric label="节点" value={cluster.nodeIds.length.toString()} />
        <Metric label="边" value={cluster.edgeIds.length.toString()} />
      </div>
      <FamilyPills families={cluster.families} testId={`cluster-families-${cluster.id}`} />
      {cluster.edgeIds.length > 0 ? (
        <div className="mt-3 text-[11px] text-[#555]">Edge IDs: {cluster.edgeIds.join(', ')}</div>
      ) : null}
      {primaryJump ? (
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => onJump(primaryJump.route, primaryJump.targetId)}
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555] hover:border-[#bbb] hover:bg-[#f7f7f7] hover:text-[#111]"
          >
            {primaryJump.label}
          </button>
        </div>
      ) : null}
    </div>
  );
}

function Metric({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="rounded border border-[#eee] bg-white px-3 py-2">
      <div className="text-[10px] uppercase tracking-wider text-[#888]">{label}</div>
      <div className={`mt-1 break-words text-[12px] font-medium text-[#111] ${mono ? 'font-mono' : ''}`}>{value}</div>
    </div>
  );
}

function FamilyPills({ families, testId }: { families: string[]; testId: string }) {
  if (families.length === 0) {
    return null;
  }

  return (
    <div className="mt-3 flex flex-wrap gap-2" data-testid={testId}>
      {families.map((family) => (
        <span
          key={`${testId}-${family}`}
          className="rounded border border-[#d0d5dd] bg-white px-2 py-1 text-[10px] font-mono text-[#344054]"
        >
          {family}
        </span>
      ))}
    </div>
  );
}

function OverviewCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white px-3 py-3 text-center">
      <div className="text-[18px] font-semibold text-[#111]">{value}</div>
      <div className="mt-1 text-[10px] uppercase tracking-wider text-[#888]">{label}</div>
    </div>
  );
}
