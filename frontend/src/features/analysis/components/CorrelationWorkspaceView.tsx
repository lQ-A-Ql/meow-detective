import type { useCorrelationWorkspaceModel } from '../use-correlation-workspace-model';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { GitBranch, Link2, ListFilter, Network, Search } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { ToggleGroup, ToggleGroupItem } from '@/app/components/ui/toggle-group';
import type { CorrelationConfidence } from '@/types/models';
import { OverviewCard } from './correlation-helpers';
import { CorrelationFamilyCoveragePanel, LeadCard } from './LeadList';
import { LeadDetailPanel } from './LeadDetail';
import { ClusterCard } from './ClusterView';

export function CorrelationWorkspaceView({ model }: { model: ReturnType<typeof useCorrelationWorkspaceModel> }) {
const { onRefresh, refreshing, t, snapshot, setSelectedLeadId, leadSearch, setLeadSearch, focusMode, setFocusMode, confidenceFilter, setConfidenceFilter, filteredLeads, jumpToTarget, selectedLead, primaryFileNode, selectedSupportingNodes, selectedLeadEdges, relatedClusters, filteredClusters, distributionNodes, reviewLeadCount, highConfidenceLeadCount, coveredFamilyCount } = model;



  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Network size={16} />
          {t('correlation.workspace.title')}
        </div>
        {onRefresh ? (
          <Tooltip content={t('correlation.workspace.refresh')}><Button aria-label={t('correlation.workspace.refresh')}
            type="button"
            variant="outline"
            size="sm"
            onClick={onRefresh}
            className="rounded-none border-forensics-border bg-forensics-surface text-[12px] hover:bg-forensics-panel-strong"
           disabled={refreshing}><ActionIcon name="refresh" /></Button></Tooltip>
        ) : null}
      </div>

      <div className="grid grid-cols-2 gap-3 xl:grid-cols-6">
        <OverviewCard label="Lead" value={snapshot.leadCount.toString()} />
        <OverviewCard label="Cluster" value={snapshot.clusterCount.toString()} />
        <OverviewCard label="Node" value={snapshot.nodeCount.toString()} />
        <OverviewCard label="Edge" value={snapshot.edgeCount.toString()} />
        <OverviewCard label={t('correlation.metrics.ruleFamilies')} value={snapshot.familyCoverage.length.toString()} />
        <OverviewCard label={t('correlation.metrics.coveredFamilies')} value={coveredFamilyCount.toString()} />
      </div>

      <CorrelationFamilyCoveragePanel items={snapshot.familyCoverage} />

      <div className="flex flex-col gap-3 rounded-none bg-transparent p-4 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex flex-1 flex-col gap-3 xl:flex-row xl:items-center">
          <label className="relative block w-full xl:max-w-[320px]">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-forensics-muted-light" />
            <Input
              value={leadSearch}
              onChange={(event) => setLeadSearch(event.target.value)}
              placeholder={t('correlation.workspace.searchPlaceholder')}
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
            <ToggleGroupItem value="all" aria-label={t('correlation.workspace.toggleAllAria')}>
              {t('timeline.all')}
            </ToggleGroupItem>
            <ToggleGroupItem value="highConfidence" aria-label={t('correlation.workspace.toggleHighConfidenceAria')}>
              {t('correlation.metrics.highConfidence')}
            </ToggleGroupItem>
            <ToggleGroupItem value="review" aria-label={t('correlation.workspace.toggleReviewAria')}>
              {t('dashboard.status.review')}
            </ToggleGroupItem>
          </ToggleGroup>

          <Select
            value={confidenceFilter}
            onValueChange={(value) => setConfidenceFilter(value as 'all' | CorrelationConfidence)}
          >
            <SelectTrigger className="w-full xl:w-[180px]" data-testid="correlation-confidence-filter">
              <SelectValue placeholder={t('correlation.workspace.confidenceAll')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('correlation.workspace.confidenceAll')}</SelectItem>
              <SelectItem value="direct">Direct</SelectItem>
              <SelectItem value="strong">Strong</SelectItem>
              <SelectItem value="weak">Weak</SelectItem>
              <SelectItem value="heuristic">Heuristic</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-wrap items-center gap-2 text-[11px] text-forensics-text-tertiary">
          <div className="flex items-center gap-2 bg-transparent px-3 py-2">
            <ListFilter size={14} className="text-forensics-muted-light" />
            <span>
              {t('correlation.workspace.summaryCount', {
                shown: filteredLeads.length,
                total: snapshot.leadCount,
              })}
            </span>
          </div>
          <div className="bg-transparent px-3 py-2">
            {t('correlation.workspace.summaryHighConfidence', { count: highConfidenceLeadCount })}
          </div>
          <div className="bg-transparent px-3 py-2">
            {t('correlation.workspace.summaryReview', { count: reviewLeadCount })}
          </div>
          <div className="bg-transparent px-3 py-2">
            {t('correlation.workspace.generatedAt', {
              time: snapshot.generatedAt.slice(0, 19).replace('T', ' '),
            })}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <div className="space-y-4">
          <div className="rounded-none bg-transparent p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
              <GitBranch size={14} />
              {t('correlation.workspace.leadsTitle')}
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
                <div className="text-[12px] text-forensics-muted">{t('correlation.workspace.leadsEmpty')}</div>
              )}
            </div>
          </div>

          <div className="rounded-none bg-transparent p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
              <Link2 size={14} />
              {t('correlation.workspace.clustersTitle')}
            </div>
            <div className="space-y-3">
              {filteredClusters.length > 0 ? (
                filteredClusters.map((cluster) => (
                  <ClusterCard key={cluster.id} cluster={cluster} onJump={jumpToTarget} />
                ))
              ) : (
                <div className="text-[12px] text-forensics-muted">{t('correlation.workspace.clustersEmpty')}</div>
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
            <div className="rounded-none bg-transparent p-4 text-[12px] text-forensics-muted">
              {t('correlation.workspace.detailEmpty')}
            </div>
          )}

          <div className="rounded-none bg-transparent p-4">
            <div className="mb-3 text-[12px] font-light text-forensics-text">{t('correlation.workspace.nodesTitle')}</div>
            <div className="space-y-2 text-[11px] text-forensics-text-tertiary">
              {distributionNodes.map((node) => (
                <div key={node.id} className="bg-transparent px-3 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <div className="truncate font-light text-forensics-text">{node.title}</div>
                    <span className="font-mono text-[10px] uppercase text-forensics-muted-light">{node.kind}</span>
                  </div>
                  {node.subtitle ? <div className="mt-1 break-all text-forensics-muted">{node.subtitle}</div> : null}
                </div>
              ))}
            </div>
          </div>

          <div className="rounded-none bg-transparent p-4">
            <div className="mb-3 text-[12px] font-light text-forensics-text">{t('correlation.workspace.boundariesTitle')}</div>
            <ul className="space-y-2 text-[11px] text-forensics-text-tertiary">
              <li>{t('correlation.workspace.boundarySourceObject')}</li>
              <li>{t('correlation.workspace.boundaryNavigational')}</li>
              <li>{t('correlation.workspace.boundaryReviewRequired')}</li>
            </ul>
          </div>
        </div>
      </div>
    </section>
  );
}
