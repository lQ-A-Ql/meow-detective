import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { CircleDot, GitBranch, HardDrive } from 'lucide-react';
import { useNavigate, type NavigateFunction } from 'react-router';
import { EmptyState } from '@/components/data-display';
import { Button } from '@/app/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { ClusterEvidenceHeader } from './ClusterEvidenceHeader';
import { EvidenceInspector } from './EvidenceInspector';
import { EvidenceTopologyPanel } from './EvidenceTopologyPanel';
import { EvidenceTrustBar } from './EvidenceTrustBar';
import { CapabilityMatrix } from './CapabilityMatrix';
import { MemberEvidenceTable } from './MemberEvidenceTable';
import { ProvenancePanel } from './ProvenancePanel';
import { TimelineContextCard } from './TimelineContextCard';
import { useLinuxClusterWorkspaceModel } from '../model/use-linux-cluster-workspace-model';
import { trustStages } from '../model/cluster-status-map';
import { ClusterOverviewPanel } from './ClusterOverviewPanel';
import { InfrastructureHeader } from './InfrastructureHeader';
import { InfrastructureInspector } from './InfrastructureInspector';
import { InfrastructureInventoryPanel } from './InfrastructureInventoryPanel';
import { NetworkTopologyPanel } from './NetworkTopologyPanel';
import { StorageEvidencePanel } from './StorageEvidencePanel';
import { WorkloadEvidencePanel } from './WorkloadEvidencePanel';

export function InfrastructurePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const model = useLinuxClusterWorkspaceModel();
  const { currentCase, summary, evidenceSets, selectedSetId, setSelectedSetId, section, setSection, selectedMember, selectedMemberIndex, setSelectedMemberIndex, capabilities, provenance, events, fallback, infrastructure, sourceMetadata, evidenceStats } = model;

  const openTimeline = () => {
    if (selectedSetId) navigate(`/timeline?scope=importSet&id=${encodeURIComponent(selectedSetId)}`);
    else navigate('/timeline');
  };

  if (!currentCase.data) return <EmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.noCase')}</EmptyState>;
  if (evidenceSets.isLoading || (selectedSetId && summary.isLoading)) return <EmptyState className="flex flex-1 items-center justify-center">{t('common.loading')}</EmptyState>;
  if (model.evidenceSets.isError || (selectedSetId && summary.isError)) return <EmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.workspace.loadFailed')}</EmptyState>;

  if (fallback || !summary.data) return <LegacyInfrastructurePage model={infrastructure} caseName={currentCase.data.name} navigate={navigate} t={t} />;

  const stages = trustStages(summary.data);
  return (
    <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-forensics-panel">
      <ClusterEvidenceHeader caseName={currentCase.data.name} sets={evidenceSets.data ?? []} selectedSetId={selectedSetId} summary={summary.data} provenance={provenance} evidenceStats={evidenceStats} onSelectSet={setSelectedSetId} onOpenTimeline={openTimeline} eventCount={events.data?.length ?? 0} t={t} />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4 lg:px-7">
        <div className="flex min-h-0 flex-col gap-4 xl:flex-row">
          <nav aria-label={t('infrastructure.workspace.navigation.label')} className="shrink-0 xl:w-56"><div className="flex gap-1 overflow-x-auto border-b border-forensics-border pb-2 xl:grid xl:gap-1 xl:border-b-0 xl:border-r xl:pb-0 xl:pr-3">{(['overview', 'members', 'topology', 'findings', 'provenance'] as const).map((item) => <button key={item} type="button" onClick={() => setSection(item)} aria-current={section === item ? 'page' : undefined} className={`whitespace-nowrap border px-3 py-2 text-left text-xs transition-colors ${section === item ? 'border-forensics-primary-blue text-forensics-primary-blue' : 'border-transparent text-forensics-muted hover:border-forensics-border hover:text-forensics-text'}`}>{t(`infrastructure.workspace.navigation.${item}`)}</button>)}</div></nav>
          <section className="min-w-0 flex-1">
            {section === 'overview' ? <div className="space-y-4"><EvidenceTrustBar stages={stages} t={t} /><CapabilityMatrix capabilities={capabilities} t={t} /><WorkloadEvidencePanel nodes={infrastructure.nodes.filter((node) => node.domain === 'analysis')} selectedId={infrastructure.selectedId} onSelect={infrastructure.setSelectedId} networkFacts={infrastructure.networkFacts} t={t} /><TimelineContextCard events={events.data ?? []} memberCount={summary.data.memberCount} onOpen={openTimeline} t={t} /></div> : null}
            {section === 'members' ? <MemberEvidenceTable members={summary.data.members} sourceMetadata={sourceMetadata} scopes={summary.data.scopes} hostFacts={infrastructure.hostFacts} networkFacts={infrastructure.networkFacts} selectedIndex={selectedMemberIndex} onSelect={setSelectedMemberIndex} t={t} /> : null}
            {section === 'topology' ? <EvidenceTopologyPanel scopes={summary.data.scopes} edges={summary.data.edges} sourceMetadata={sourceMetadata} hostFacts={infrastructure.hostFacts} networkFacts={infrastructure.networkFacts} t={t} /> : null}
            {section === 'findings' ? <FindingsPanel summary={summary.data} onOpenTimeline={openTimeline} t={t} /> : null}
            {section === 'provenance' ? <ProvenancePanel summary={summary.data} provenance={provenance} t={t} /> : null}
          </section>
          <EvidenceInspector member={selectedMember} onClose={() => setSelectedMemberIndex(undefined)} t={t} />
        </div>
      </div>
    </main>
  );
}

function FindingsPanel({ summary, onOpenTimeline, t }: { summary: NonNullable<ReturnType<typeof useLinuxClusterWorkspaceModel>['summary']['data']>; onOpenTimeline: () => void; t: TFunction }) {
  const diagnostics = useMemo(() => summary.diagnostics.slice(0, 5), [summary.diagnostics]);
  return <section className="border border-forensics-border bg-forensics-surface p-5"><div className="flex flex-wrap items-start justify-between gap-3"><div><h2 className="font-serif text-[15px] font-light text-forensics-text">{t('infrastructure.workspace.findings.title')}</h2><p className="mt-1 text-[11px] leading-5 text-forensics-muted">{t('infrastructure.workspace.findings.description')}</p></div><Button type="button" variant="forensicsOutline" size="sm" onClick={onOpenTimeline}>{t('infrastructure.workspace.findings.openTimeline')}</Button></div><div className="mt-4 grid gap-3 sm:grid-cols-3"><div className="border border-forensics-border p-3"><div className="text-[10px] text-forensics-muted">{t('infrastructure.workspace.findings.supported')}</div><div className="mt-1 text-xl font-light text-forensics-text">{summary.scopes.filter((scope) => scope.identityState === 'proven' || scope.status === 'complete').length}</div></div><div className="border border-forensics-border p-3"><div className="text-[10px] text-forensics-muted">{t('infrastructure.workspace.findings.candidate')}</div><div className="mt-1 text-xl font-light text-forensics-text">{summary.scopes.filter((scope) => scope.identityState === 'candidate' || scope.status === 'partial').length}</div></div><div className="border border-forensics-border p-3"><div className="text-[10px] text-forensics-muted">{t('infrastructure.workspace.findings.limited')}</div><div className="mt-1 text-xl font-light text-forensics-text">{diagnostics.length}</div></div></div>{diagnostics.length ? <div className="mt-4 space-y-2 text-xs text-forensics-muted">{diagnostics.map((diagnostic) => <div key={diagnostic} className="border-l-2 border-forensics-border-strong pl-3">{diagnostic}</div>)}</div> : <div className="mt-4 text-xs text-forensics-muted">{t('infrastructure.workspace.findings.empty')}</div>}</section>;
}

function LegacyInfrastructurePage({ model, caseName, navigate, t }: { model: ReturnType<typeof useLinuxClusterWorkspaceModel>['infrastructure']; caseName: string; navigate: NavigateFunction; t: TFunction }) {
  const { graph, nodes, edges, visibleNodes, nodeNames, networkTopology, networkFacts, hostFacts, query, selected, selectedId, setQuery, setSelectedId } = model;
  if (graph.isLoading) return <EmptyState className="flex flex-1 items-center justify-center">{t('common.loading')}</EmptyState>;
  if (!graph.data || nodes.length === 0) return <EmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.empty')}</EmptyState>;
  const workloads = nodes.filter((node) => node.domain === 'analysis');
  const storage = nodes.filter((node) => node.domain === 'storage');
  return <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-forensics-panel"><InfrastructureHeader caseName={caseName} nodes={nodes} relationCount={edges.length} networkLinkCount={networkTopology.edges.length} t={t} /><Tabs defaultValue="overview" className="min-h-0 flex-1 overflow-hidden px-5 py-4 lg:px-7"><TabsList><TabsTrigger value="overview"><CircleDot size={14} />{t('infrastructure.tabs.overview')}</TabsTrigger><TabsTrigger value="topology"><GitBranch size={14} />{t('infrastructure.tabs.topology')}</TabsTrigger><TabsTrigger value="workloads">{t('infrastructure.tabs.workloads')}</TabsTrigger><TabsTrigger value="storage">{t('infrastructure.tabs.storage')}</TabsTrigger><TabsTrigger value="inventory"><HardDrive size={14} />{t('infrastructure.tabs.inventory')}</TabsTrigger></TabsList><TabsContent value="overview"><ClusterOverviewPanel nodes={nodes} edges={edges} hostFacts={hostFacts} networkFacts={networkFacts} t={t} /></TabsContent><TabsContent value="topology"><NetworkTopologyPanel nodes={networkTopology.nodes} edges={networkTopology.edges} networkFacts={networkFacts} nodeNames={nodeNames} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent><TabsContent value="workloads"><WorkloadEvidencePanel nodes={workloads} selectedId={selectedId} onSelect={setSelectedId} networkFacts={networkFacts} t={t} /></TabsContent><TabsContent value="storage"><StorageEvidencePanel nodes={storage} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent><TabsContent value="inventory"><InfrastructureInventoryPanel nodes={visibleNodes} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent></Tabs>{selected ? <InfrastructureInspector node={selected} edges={edges} onOpenFiles={() => navigate('/files')} onClose={() => setSelectedId(undefined)} t={t} /> : null}<input aria-label={t('infrastructure.actions.search')} value={query} onChange={(event) => setQuery(event.target.value)} className="sr-only" /></main>;
}
