import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { Boxes, Copy, Database, FileSearch, Network, Server, ShieldCheck } from 'lucide-react';
import { useNavigate } from 'react-router';
import { Button } from '@/app/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { useCurrentCase } from '@/features/case/hooks';
import { getInfrastructureGraph } from '@/lib/api/infrastructure';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';

type Domain = InfrastructureGraphNode['domain'];

const DOMAIN_ORDER: Domain[] = ['environment', 'storage', 'analysis'];

export function LinuxInfrastructure() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const currentCase = useCurrentCase();
  const graph = useQuery({
    queryKey: ['infrastructure', 'graph', currentCase.data?.id ?? null],
    queryFn: getInfrastructureGraph,
    enabled: Boolean(currentCase.data),
  });
  const [selectedId, setSelectedId] = useState<string>();
  const data = graph.data;
  const nodes = data?.nodes ?? [];
  const edges = data?.edges ?? [];
  const selected = nodes.find((node) => node.id === selectedId);
  const byDomain = useMemo(
    () => Object.fromEntries(DOMAIN_ORDER.map((domain) => [domain, nodes.filter((node) => node.domain === domain)])) as Record<Domain, InfrastructureGraphNode[]>,
    [nodes],
  );
  const workloadNodes = nodes.filter((node) => node.domain === 'analysis' && isRuntimeEvidence(node.kind));
  const serviceNodes = nodes.filter((node) => node.domain === 'analysis' && isServiceEvidence(node.kind));

  if (!currentCase.data) return <EmptyState text={t('infrastructure.noCase')} />;
  if (graph.isLoading) return <EmptyState text={t('common.loading')} />;
  if (!data || nodes.length === 0) return <EmptyState text={t('infrastructure.empty')} />;

  return (
    <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-forensics-panel">
      <header className="shrink-0 border-b border-forensics-border bg-forensics-surface px-6 py-4">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <h1 className="font-serif text-xl font-light text-forensics-text">{t('infrastructure.title')}</h1>
            <p className="mt-1 text-xs text-forensics-muted">{t('infrastructure.subtitle')}</p>
          </div>
          <div className="font-mono text-[11px] text-forensics-muted">{currentCase.data.name}</div>
        </div>
        <div className="mt-4 grid grid-cols-2 border border-forensics-border sm:grid-cols-4">
          <Metric icon={<Server size={14} />} label={t('infrastructure.metrics.environments')} value={byDomain.environment.length} />
          <Metric icon={<Database size={14} />} label={t('infrastructure.metrics.storage')} value={byDomain.storage.length} />
          <Metric icon={<Boxes size={14} />} label={t('infrastructure.metrics.workloads')} value={workloadNodes.length} />
          <Metric icon={<Network size={14} />} label={t('infrastructure.metrics.relations')} value={edges.length} />
        </div>
      </header>

      <Tabs defaultValue="overview" className="min-h-0 flex-1 overflow-hidden px-6 py-4">
        <TabsList className="max-w-full overflow-x-auto">
          <TabsTrigger value="overview">{t('infrastructure.tabs.overview')}</TabsTrigger>
          <TabsTrigger value="topology">{t('infrastructure.tabs.topology')}</TabsTrigger>
          <TabsTrigger value="nodes">{t('infrastructure.tabs.nodes')}</TabsTrigger>
          <TabsTrigger value="workloads">{t('infrastructure.tabs.workloads')}</TabsTrigger>
          <TabsTrigger value="storage">{t('infrastructure.tabs.storage')}</TabsTrigger>
          <TabsTrigger value="evidence">{t('infrastructure.tabs.evidence')}</TabsTrigger>
        </TabsList>
        <TabsContent value="overview" className="h-[calc(100%-46px)] overflow-auto pt-3">
          <Overview domains={byDomain} edges={edges} onSelect={setSelectedId} t={t} />
        </TabsContent>
        <TabsContent value="topology" className="h-[calc(100%-46px)] overflow-auto pt-3">
          <Topology domains={byDomain} edges={edges} selectedId={selectedId} onSelect={setSelectedId} t={t} />
        </TabsContent>
        <TabsContent value="nodes" className="h-[calc(100%-46px)] overflow-auto pt-3">
          <NodeTable nodes={byDomain.environment} onSelect={setSelectedId} t={t} />
        </TabsContent>
        <TabsContent value="workloads" className="h-[calc(100%-46px)] overflow-auto pt-3">
          <WorkloadView workloads={workloadNodes} services={serviceNodes} onSelect={setSelectedId} t={t} />
        </TabsContent>
        <TabsContent value="storage" className="h-[calc(100%-46px)] overflow-auto pt-3">
          <NodeTable nodes={byDomain.storage} onSelect={setSelectedId} t={t} />
        </TabsContent>
        <TabsContent value="evidence" className="h-[calc(100%-46px)] overflow-auto pt-3">
          <EvidenceTable nodes={byDomain.analysis} onSelect={setSelectedId} t={t} />
        </TabsContent>
      </Tabs>

      <Inspector node={selected} edges={edges} onOpenFiles={() => navigate('/files')} t={t} />
    </main>
  );
}

function Overview({ domains, edges, onSelect, t }: { domains: Record<Domain, InfrastructureGraphNode[]>; edges: InfrastructureGraphEdge[]; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]"><section className="border border-forensics-border bg-forensics-surface"><SectionTitle icon={<Network size={14} />} title={t('infrastructure.sections.domains')} /><div className="grid md:grid-cols-3">{DOMAIN_ORDER.map((domain) => <div key={domain} className="border-b border-r border-forensics-border p-4 last:border-r-0 md:border-b-0"><div className="mb-3 font-mono text-[10px] uppercase text-forensics-muted">{domain}</div><div className="space-y-1">{domains[domain].slice(0, 8).map((node) => <button key={node.id} onClick={() => onSelect(node.id)} className="block w-full truncate border border-transparent px-2 py-1 text-left text-xs text-forensics-text hover:border-forensics-border hover:bg-forensics-hover">{node.name}</button>)}</div></div>)}</div></section><RelationList edges={edges} t={t} /></div>;
}

function Topology({ domains, edges, selectedId, onSelect, t }: { domains: Record<Domain, InfrastructureGraphNode[]>; edges: InfrastructureGraphEdge[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]"><section className="overflow-auto border border-forensics-border bg-forensics-surface p-4"><div className="grid min-w-[740px] grid-cols-3 gap-5">{DOMAIN_ORDER.map((domain) => <div key={domain}><div className="mb-3 border-b border-forensics-border pb-2 font-mono text-[10px] uppercase text-forensics-muted">{domain}</div><div className="space-y-2">{domains[domain].map((node) => <button key={node.id} onClick={() => onSelect(node.id)} className={`w-full border p-3 text-left ${selectedId === node.id ? 'border-forensics-text bg-forensics-hover' : 'border-forensics-border bg-forensics-panel hover:border-forensics-border-strong'}`}><div className="truncate text-xs text-forensics-text">{node.name}</div><div className="mt-1 font-mono text-[10px] text-forensics-muted">{node.kind} · {node.confidence}</div></button>)}</div></div>)}</div></section><RelationList edges={edges} t={t} /></div>;
}

function WorkloadView({ workloads, services, onSelect, t }: { workloads: InfrastructureGraphNode[]; services: InfrastructureGraphNode[]; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <div className="space-y-4"><div className="border border-forensics-border bg-forensics-surface px-3 py-2 text-xs text-forensics-muted">{t('infrastructure.workloadHint')}</div><div className="grid gap-4 xl:grid-cols-2"><EvidenceTable title={t('infrastructure.sections.workloads')} nodes={workloads} onSelect={onSelect} t={t} /><EvidenceTable title={t('infrastructure.sections.nodes')} nodes={services} onSelect={onSelect} t={t} /></div></div>;
}

function NodeTable({ nodes, onSelect, t }: { nodes: InfrastructureGraphNode[]; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) { return <Table title={t('infrastructure.sections.nodes')} nodes={nodes} onSelect={onSelect} t={t} />; }
function EvidenceTable({ nodes, onSelect, t, title }: { nodes: InfrastructureGraphNode[]; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t']; title?: string }) { return <Table title={title ?? t('infrastructure.sections.evidence')} nodes={nodes} onSelect={onSelect} t={t} />; }

function Table({ title, nodes, onSelect, t }: { title: string; nodes: InfrastructureGraphNode[]; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <section className="overflow-hidden border border-forensics-border bg-forensics-surface"><SectionTitle icon={<Server size={14} />} title={title} /><div className="overflow-auto"><table className="w-full min-w-[620px] text-left text-xs"><thead className="border-y border-forensics-border bg-forensics-panel font-mono text-[10px] uppercase text-forensics-muted"><tr><th className="px-3 py-2">{t('infrastructure.columns.name')}</th><th className="px-3 py-2">{t('infrastructure.columns.type')}</th><th className="px-3 py-2">{t('infrastructure.columns.status')}</th><th className="px-3 py-2">{t('infrastructure.columns.confidence')}</th></tr></thead><tbody>{nodes.map((node) => <tr key={node.id} onClick={() => onSelect(node.id)} className="cursor-pointer border-b border-forensics-border last:border-0 hover:bg-forensics-hover"><td className="max-w-80 truncate px-3 py-2 text-forensics-text">{node.name}</td><td className="px-3 py-2 font-mono text-[11px] text-forensics-muted">{node.kind}</td><td className="px-3 py-2"><State value={node.status} /></td><td className="px-3 py-2"><State value={node.confidence} /></td></tr>)}</tbody></table></div>{nodes.length === 0 ? <EmptyState text={t('infrastructure.values.notFound')} compact /> : null}</section>;
}

function RelationList({ edges, t }: { edges: InfrastructureGraphEdge[]; t: ReturnType<typeof useTranslation>['t'] }) { return <section className="overflow-hidden border border-forensics-border bg-forensics-surface"><SectionTitle icon={<ShieldCheck size={14} />} title={t('infrastructure.sections.relations')} /><div className="max-h-[540px] overflow-auto">{edges.map((edge, index) => <div key={`${edge.sourceId}-${edge.targetId}-${index}`} className="border-b border-forensics-border px-3 py-2 last:border-0"><div className="font-mono text-[10px] text-forensics-muted">{edge.relationKind}</div><div className="mt-1 truncate text-xs text-forensics-text">{shortId(edge.sourceId)} → {shortId(edge.targetId)}</div></div>)}</div></section>; }

function Inspector({ node, edges, onOpenFiles, t }: { node?: InfrastructureGraphNode; edges: InfrastructureGraphEdge[]; onOpenFiles: () => void; t: ReturnType<typeof useTranslation>['t'] }) { const provenance = parseJson(node?.provenanceJson); const linked = node ? edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id) : []; return <aside className="fixed bottom-0 right-0 top-[calc(100%-230px)] z-20 w-full overflow-auto border-t border-forensics-border bg-forensics-surface p-4 shadow-lg md:w-[420px]"><div className="mb-3 flex items-center justify-between"><SectionTitle icon={<FileSearch size={14} />} title={t('infrastructure.sections.inspector')} compact />{node ? <Button size="xs" variant="forensicsOutline" onClick={() => navigator.clipboard?.writeText(node.id)} title={t('infrastructure.actions.copyId')}><Copy size={12} /></Button> : null}</div>{node ? <div className="space-y-3"><div><div className="text-sm text-forensics-text">{node.name}</div><div className="mt-1 font-mono text-[10px] text-forensics-muted">{node.id}</div></div><div className="grid grid-cols-2 gap-px border border-forensics-border bg-forensics-border"><Metric label={t('infrastructure.columns.type')} value={node.kind} /><Metric label={t('infrastructure.columns.confidence')} value={node.confidence} /></div><pre className="overflow-auto border border-forensics-border bg-forensics-panel p-2 font-mono text-[10px] text-forensics-muted">{JSON.stringify(provenance, null, 2)}</pre><div className="text-[11px] text-forensics-muted">{linked.length} {t('infrastructure.metrics.relations')}</div>{provenance.dataSourceId ? <Button size="sm" variant="forensicsOutline" onClick={onOpenFiles}>{t('infrastructure.actions.openFiles')}</Button> : null}</div> : <div className="text-xs text-forensics-muted">{t('infrastructure.values.unselected')}</div>}</aside>; }

function Metric({ icon, label, value }: { icon?: React.ReactNode; label: string; value: string | number }) { return <div className="min-w-0 bg-forensics-surface px-3 py-3"><div className="flex items-center gap-2 text-[10px] uppercase text-forensics-muted">{icon}{label}</div><div className="mt-1 truncate font-mono text-lg text-forensics-text">{value}</div></div>; }
function SectionTitle({ icon, title, compact }: { icon: React.ReactNode; title: string; compact?: boolean }) { return <div className={`${compact ? '' : 'border-b border-forensics-border px-3 py-2'} flex items-center gap-2 text-[11px] font-light uppercase tracking-wide text-forensics-text`}><span className="text-forensics-muted">{icon}</span>{title}</div>; }
function State({ value }: { value: string }) { return <span className="border border-forensics-border bg-forensics-panel px-1.5 py-0.5 font-mono text-[10px] text-forensics-text">{value}</span>; }
function EmptyState({ text, compact }: { text: string; compact?: boolean }) { return <div className={`${compact ? 'p-6' : 'flex flex-1 items-center justify-center'} text-sm text-forensics-muted`}>{text}</div>; }
function parseJson(value?: string) { try { return value ? JSON.parse(value) : {}; } catch { return { raw: value }; } }
function shortId(value: string) { return value.length > 40 ? `${value.slice(0, 37)}…` : value; }
function isRuntimeEvidence(kind: string) { return ['static_pod_manifest', 'pod_log', 'container_log', 'runtime_metadata', 'pod_volume'].includes(kind); }
function isServiceEvidence(kind: string) { return ['kubelet_config', 'kubeconfig', 'audit_log', 'cni_config'].includes(kind); }
