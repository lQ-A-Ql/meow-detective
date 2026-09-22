import { useTranslation } from 'react-i18next';
import { Fragment } from 'react';
import {
  Boxes, ChevronRight, CircleDot, Copy, Database, ExternalLink, GitBranch,
  HardDrive, Layers3, Network, Search, Server,
} from 'lucide-react';
import { useNavigate } from 'react-router';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/app/components/ui/table';
import { StatusBadge } from '@/components/status/StatusBadge';
import { EmptyState as SharedEmptyState, KeyValueField, MetricCard, SectionHeader } from '@/components/data-display';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { useInfrastructureWorkspace } from '@/features/infrastructure/hooks/useInfrastructureWorkspace';
import { NetworkTopologyCanvas } from '@/features/infrastructure/components/NetworkTopologyCanvas';
import { domainLabel, kindLabel, relationLabel, stateLabel } from '@/features/infrastructure/logic/labels';
import { INFRASTRUCTURE_DOMAIN_ORDER } from '@/features/infrastructure/types';

type Domain = InfrastructureGraphNode['domain'];
const DOMAIN_ORDER = INFRASTRUCTURE_DOMAIN_ORDER;

export function InfrastructurePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { currentCase, graph, nodes, edges, domains: byDomain, visibleNodes, nodeNames, networkTopology, hostFacts, query, selected, selectedId, setQuery, setSelectedId } = useInfrastructureWorkspace();

  if (!currentCase.data) return <SharedEmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.noCase')}</SharedEmptyState>;
  if (graph.isLoading) return <SharedEmptyState className="flex flex-1 items-center justify-center">{t('common.loading')}</SharedEmptyState>;
  if (!graph.data || nodes.length === 0) return <SharedEmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.empty')}</SharedEmptyState>;

  return (
    <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-forensics-panel">
      <header className="shrink-0 border-b border-forensics-border bg-forensics-surface px-5 py-4 lg:px-7">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="mb-1 flex items-center gap-2 text-[10px] uppercase tracking-[0.16em] text-forensics-muted">
              <Layers3 size={13} /> <span>{t('infrastructure.eyebrow')}</span>
            </div>
            <h1 className="text-xl font-light text-forensics-text">{t('infrastructure.title')}</h1>
            <p className="mt-1 max-w-2xl text-xs leading-5 text-forensics-muted">{t('infrastructure.subtitle')}</p>
          </div>
          <div className="max-w-full truncate border-l border-forensics-border pl-3 text-right text-xs text-forensics-text">
            <div className="text-[10px] uppercase tracking-wide text-forensics-muted">{t('infrastructure.caseLabel')}</div>
            <div className="mt-1 truncate">{currentCase.data.name}</div>
          </div>
        </div>
        <div className="mt-5 grid grid-cols-2 border-y border-forensics-border sm:grid-cols-5">
          <MetricCard icon={Server} label={t('infrastructure.metrics.environments')} value={byDomain.environment.length} size="sm" className="border-0 border-r" />
          <MetricCard icon={Database} label={t('infrastructure.metrics.storage')} value={byDomain.storage.length} size="sm" className="border-0 border-r" />
          <MetricCard icon={Boxes} label={t('infrastructure.metrics.workloads')} value={byDomain.analysis.length} size="sm" className="border-0 border-r" />
          <MetricCard icon={Network} label={t('infrastructure.metrics.relations')} value={edges.length} size="sm" className="border-0" />
          <MetricCard icon={GitBranch} label={t('infrastructure.metrics.hostLinks')} value={networkTopology.edges.length} size="sm" className="border-0" />
        </div>
        <ClusterIdentitySummary nodes={nodes} hostFacts={hostFacts} t={t} />
      </header>

      <Tabs defaultValue="overview" className="min-h-0 flex-1 overflow-hidden px-5 py-4 lg:px-7">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-forensics-border">
          <TabsList className="max-w-full overflow-x-auto">
            <TabsTrigger value="overview"><CircleDot size={14} />{t('infrastructure.tabs.overview')}</TabsTrigger>
            <TabsTrigger value="topology"><GitBranch size={14} />{t('infrastructure.tabs.topology')}</TabsTrigger>
            <TabsTrigger value="inventory"><HardDrive size={14} />{t('infrastructure.tabs.inventory')}</TabsTrigger>
          </TabsList>
          <div className="relative mb-1 w-full sm:w-64">
            <Search className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-forensics-muted" size={14} />
            <Input aria-label={t('infrastructure.actions.search')} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t('infrastructure.searchPlaceholder')} variant="forensics" inputSize="compact" className="pl-7" />
          </div>
        </div>
        <TabsContent value="overview" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><Overview domains={byDomain} edges={edges} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="topology" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><Topology nodes={networkTopology.nodes} edges={networkTopology.edges} nodeNames={nodeNames} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="inventory" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><Inventory nodes={visibleNodes} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
      </Tabs>
      {selected ? <Inspector node={selected} edges={edges} onOpenFiles={() => navigate('/files')} onClose={() => setSelectedId(undefined)} t={t} /> : null}
    </main>
  );
}

function Overview({ domains, edges, selectedId, onSelect, t }: { domains: Record<Domain, InfrastructureGraphNode[]>; edges: InfrastructureGraphEdge[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  const nodeNames = new Map([...DOMAIN_ORDER.flatMap((domain) => domains[domain])].map((node) => [node.id, node.name]));
  return <div className="space-y-4"><section className="border border-forensics-border bg-forensics-surface"><SectionHeader icon={Layers3} title={t('infrastructure.sections.layers')} subtitle={t('infrastructure.layersDescription')} className="border-0 px-4 py-3" /><div className="grid lg:grid-cols-3">{DOMAIN_ORDER.map((domain) => <LayerColumn key={domain} domain={domain} nodes={domains[domain]} selectedId={selectedId} onSelect={onSelect} t={t} />)}</div></section><section className="border border-forensics-border bg-forensics-surface"><SectionHeader icon={Network} title={t('infrastructure.sections.relations')} subtitle={t('infrastructure.relationsDescription')} className="border-0 px-4 py-3" /><RelationList edges={edges.slice(0, 8)} nodeNames={nodeNames} t={t} /></section></div>;
}

function LayerColumn({ domain, nodes, selectedId, onSelect, t }: { domain: Domain; nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <div className="border-b border-forensics-border p-4 last:border-b-0 lg:border-b-0 lg:border-r lg:last:border-r-0"><div className="flex items-start justify-between gap-3"><div><div className="text-sm text-forensics-text">{domainLabel(domain, t)}</div><div className="mt-1 text-[11px] leading-4 text-forensics-muted">{t(`infrastructure.domainDescriptions.${domain}`)}</div></div><Badge variant="outline">{nodes.length}</Badge></div><div className="mt-4 divide-y divide-forensics-border border-y border-forensics-border">{nodes.slice(0, 6).map((node) => <NodeListItem key={node.id} node={node} selected={selectedId === node.id} onSelect={onSelect} t={t} />)}</div>{nodes.length > 6 ? <div className="mt-3 text-[11px] text-forensics-muted">{t('infrastructure.moreObjects', { count: nodes.length - 6 })}</div> : null}{nodes.length === 0 ? <div className="mt-4 text-xs text-forensics-muted">{t('infrastructure.values.notFound')}</div> : null}</div>;
}

function Topology({ nodes, edges, nodeNames, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; edges: InfrastructureGraphEdge[]; nodeNames: Map<string, string>; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]"><section className="overflow-hidden border border-forensics-border bg-forensics-surface p-4"><div className="mb-4 flex items-center gap-2 text-xs text-forensics-muted"><Network size={14} />{t('infrastructure.topologyHint')}</div><NetworkTopologyCanvas nodes={nodes} edges={edges} selectedId={selectedId} onSelect={onSelect} t={t} /></section><RelationList edges={edges} nodeNames={nodeNames} t={t} /></div>;
}

function ClusterIdentitySummary({ nodes, hostFacts, t }: { nodes: InfrastructureGraphNode[]; hostFacts: Map<string, { hostname?: string; operatingSystem?: string; operatingSystemVersion?: string; kernelVersion?: string }>; t: ReturnType<typeof useTranslation>['t'] }) {
  const environment = nodes.filter((node) => node.domain === 'environment');
  const hosts = nodes.filter((node) => node.kind === 'physical_host');
  const kinds = [...new Set(environment.map((node) => kindLabel(node.kind, t)))];
  const sourceId = hosts.map((node) => parseJson(node.provenanceJson).dataSourceId).find((value): value is string => typeof value === 'string');
  const fact = sourceId ? hostFacts.get(sourceId) : undefined;
  const version = [fact?.operatingSystem, fact?.operatingSystemVersion, fact?.kernelVersion].filter(Boolean).join(' / ');
  return <section className="mt-4 border border-forensics-border bg-forensics-panel px-4 py-3"><div className="mb-3 text-xs font-light text-forensics-text">{t('infrastructure.clusterInfo.title')}</div><div className="grid gap-3 text-xs sm:grid-cols-5"><KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.types')} value={kinds.join(' / ') || t('infrastructure.values.unavailable')} /><KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.hosts')} value={hosts.length.toString()} /><KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.identity')} value={stateLabel(environment[0]?.confidence ?? 'unproven', t)} /><KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.hostname')} value={fact?.hostname ?? t('infrastructure.values.unavailable')} /><KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.version')} value={version || t('infrastructure.values.unavailable')} /></div><div className="mt-4 overflow-x-auto border-t border-forensics-border pt-3"><div className="mb-2 text-[11px] text-forensics-muted">{t('infrastructure.clusterInfo.hostEvidence')}</div><div className="grid min-w-[640px] grid-cols-[minmax(180px,1fr)_minmax(140px,1fr)_minmax(260px,2fr)] gap-x-4 gap-y-2 text-[11px]"><span className="text-forensics-muted">{t('infrastructure.clusterInfo.host')}</span><span className="text-forensics-muted">{t('infrastructure.clusterInfo.hostname')}</span><span className="text-forensics-muted">{t('infrastructure.clusterInfo.version')}</span>{hosts.map((host) => { const hostSourceId = parseJson(host.provenanceJson).dataSourceId; const hostFact = typeof hostSourceId === 'string' ? hostFacts.get(hostSourceId) : undefined; const hostVersion = [hostFact?.operatingSystem, hostFact?.operatingSystemVersion, hostFact?.kernelVersion].filter(Boolean).join(' / '); return <Fragment key={host.id}><span className="truncate text-forensics-text" title={host.name}>{host.name}</span><span className="truncate text-forensics-text-secondary">{hostFact?.hostname ?? t('infrastructure.values.unavailable')}</span><span className="truncate font-mono text-forensics-text-secondary">{hostVersion || t('infrastructure.values.unavailable')}</span></Fragment>; })}</div></div></section>;
}

function Inventory({ nodes, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <section className="overflow-hidden border border-forensics-border bg-forensics-surface"><SectionHeader icon={HardDrive} title={t('infrastructure.sections.inventory')} subtitle={t('infrastructure.inventoryDescription', { count: nodes.length })} className="border-0 px-4 py-3" /><div className="overflow-auto"><Table className="min-w-[700px] text-left text-xs"><TableHeader><TableRow><TableHead>{t('infrastructure.columns.name')}</TableHead><TableHead>{t('infrastructure.columns.layer')}</TableHead><TableHead>{t('infrastructure.columns.type')}</TableHead><TableHead>{t('infrastructure.columns.status')}</TableHead><TableHead>{t('infrastructure.columns.confidence')}</TableHead></TableRow></TableHeader><TableBody>{nodes.map((node) => <TableRow key={node.id} data-state={selectedId === node.id ? 'selected' : undefined} onClick={() => onSelect(node.id)} className="cursor-pointer"><TableCell className="max-w-[280px] truncate text-forensics-text">{node.name}</TableCell><TableCell className="text-forensics-muted">{domainLabel(node.domain, t)}</TableCell><TableCell className="text-forensics-muted">{kindLabel(node.kind, t)}</TableCell><TableCell><StatusBadge label={stateLabel(node.status, t)} variant={statusVariant(node.status)} /></TableCell><TableCell><StatusBadge label={stateLabel(node.confidence, t)} variant={statusVariant(node.confidence)} /></TableCell></TableRow>)}</TableBody></Table></div>{nodes.length === 0 ? <SharedEmptyState>{t('infrastructure.values.notFound')}</SharedEmptyState> : null}</section>;
}

function RelationList({ edges, nodeNames, t }: { edges: InfrastructureGraphEdge[]; nodeNames: Map<string, string>; t: ReturnType<typeof useTranslation>['t'] }) {
  return <section className="overflow-hidden border border-forensics-border bg-forensics-surface"><SectionHeader icon={Network} title={t('infrastructure.sections.relations')} subtitle={t('infrastructure.relationsDescription')} className="border-0 px-4 py-3" /><div className="max-h-[540px] divide-y divide-forensics-border overflow-auto">{edges.map((edge, index) => <div key={`${edge.sourceId}-${edge.targetId}-${index}`} className="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-center"><div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.sourceId) ?? edge.sourceId}>{nodeNames.get(edge.sourceId) ?? t('infrastructure.values.unknown')}</div><div className="flex items-center gap-1 text-[10px] text-forensics-muted"><ChevronRight size={13} /><span>{relationLabel(edge.relationKind, t)}</span></div><div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.targetId) ?? edge.targetId}>{nodeNames.get(edge.targetId) ?? t('infrastructure.values.unknown')}</div></div>)}</div>{edges.length === 0 ? <SharedEmptyState>{t('infrastructure.values.notFound')}</SharedEmptyState> : null}</section>;
}

function Inspector({ node, edges, onOpenFiles, onClose, t }: { node: InfrastructureGraphNode; edges: InfrastructureGraphEdge[]; onOpenFiles: () => void; onClose: () => void; t: ReturnType<typeof useTranslation>['t'] }) {
  const provenance = parseJson(node.provenanceJson);
  const linked = edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id);
  const sourceId = typeof provenance.dataSourceId === 'string' ? provenance.dataSourceId : undefined;
  return <aside className="shrink-0 border-t border-forensics-border bg-forensics-surface px-5 py-4 lg:px-7"><div className="mx-auto flex max-w-[1500px] flex-col gap-4 lg:flex-row lg:items-start lg:justify-between"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="text-sm text-forensics-text">{node.name}</span><StatusBadge label={stateLabel(node.status, t)} variant={statusVariant(node.status)} /></div><div className="mt-1 text-xs text-forensics-muted">{domainLabel(node.domain, t)} · {kindLabel(node.kind, t)} · {t('infrastructure.inspector.linkedRelations', { count: linked.length })}</div><div className="mt-3 grid gap-2 text-xs sm:grid-cols-3"><KeyValueField layout="inline" label={t('infrastructure.columns.confidence')} value={stateLabel(node.confidence, t)} /><KeyValueField layout="inline" label={t('infrastructure.inspector.source')} value={sourceId ?? t('infrastructure.values.unavailable')} /><KeyValueField layout="inline" label={t('infrastructure.inspector.parser')} value={typeof provenance.parser === 'string' ? provenance.parser : t('infrastructure.values.unavailable')} /></div></div><div className="flex shrink-0 items-center gap-2"><Button size="xs" variant="forensicsOutline" onClick={() => navigator.clipboard?.writeText(node.id)} title={t('infrastructure.actions.copyId')}><Copy size={12} />{t('infrastructure.actions.copyId')}</Button>{sourceId ? <Button size="xs" variant="forensicsOutline" onClick={onOpenFiles}><ExternalLink size={12} />{t('infrastructure.actions.openFiles')}</Button> : null}<Button size="xs" variant="forensicsGhost" onClick={onClose}>{t('infrastructure.actions.close')}</Button></div></div></aside>;
}

function NodeListItem({ node, selected, onSelect, t }: { node: InfrastructureGraphNode; selected: boolean; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) { return <Button size="inline" variant="forensicsGhost" onClick={() => onSelect(node.id)} className={`flex w-full items-center justify-between gap-3 border-l-2 py-2 text-left hover:text-forensics-primary-blue ${selected ? 'border-forensics-primary-blue bg-forensics-hover' : 'border-transparent'}`}><span className="min-w-0 truncate text-xs text-forensics-text">{node.name}</span><span className="shrink-0 text-[10px] text-forensics-muted">{kindLabel(node.kind, t)}</span></Button>; }
function statusVariant(value: string) { return value === 'ready' || value === 'complete' || value === 'verified' ? 'default' : 'outline'; }

function parseJson(value: string) { try { return JSON.parse(value) as Record<string, unknown>; } catch { return {}; } }
