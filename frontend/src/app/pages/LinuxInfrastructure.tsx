import { useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
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
  const [query, setQuery] = useState('');
  const data = graph.data;
  const nodes = data?.nodes ?? [];
  const edges = data?.edges ?? [];
  const selected = nodes.find((node) => node.id === selectedId);
  const byDomain = useMemo(() => groupByDomain(nodes), [nodes]);
  const visibleNodes = useMemo(
    () => nodes.filter((node) => matchesQuery(node, query, t)),
    [nodes, query, t],
  );
  const nodeNames = useMemo(() => new Map(nodes.map((node) => [node.id, node.name])), [nodes]);

  if (!currentCase.data) return <EmptyState text={t('infrastructure.noCase')} />;
  if (graph.isLoading) return <EmptyState text={t('common.loading')} />;
  if (!data || nodes.length === 0) return <EmptyState text={t('infrastructure.empty')} />;

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
        <div className="mt-5 grid grid-cols-2 border-y border-forensics-border sm:grid-cols-4">
          <Metric icon={<Server size={14} />} label={t('infrastructure.metrics.environments')} value={byDomain.environment.length} />
          <Metric icon={<Database size={14} />} label={t('infrastructure.metrics.storage')} value={byDomain.storage.length} />
          <Metric icon={<Boxes size={14} />} label={t('infrastructure.metrics.workloads')} value={byDomain.analysis.length} />
          <Metric icon={<Network size={14} />} label={t('infrastructure.metrics.relations')} value={edges.length} />
        </div>
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
        <TabsContent value="overview" className="h-[calc(100%-46px)] overflow-auto pt-4"><Overview domains={byDomain} edges={edges} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="topology" className="h-[calc(100%-46px)] overflow-auto pt-4"><Topology domains={byDomain} edges={edges} nodeNames={nodeNames} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="inventory" className="h-[calc(100%-46px)] overflow-auto pt-4"><Inventory nodes={visibleNodes} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
      </Tabs>
      {selected ? <Inspector node={selected} edges={edges} onOpenFiles={() => navigate('/files')} onClose={() => setSelectedId(undefined)} t={t} /> : null}
    </main>
  );
}

function Overview({ domains, edges, selectedId, onSelect, t }: { domains: Record<Domain, InfrastructureGraphNode[]>; edges: InfrastructureGraphEdge[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  const nodeNames = new Map([...DOMAIN_ORDER.flatMap((domain) => domains[domain])].map((node) => [node.id, node.name]));
  return <div className="space-y-4"><section className="border border-forensics-border bg-forensics-surface"><SectionHeading icon={<Layers3 size={15} />} title={t('infrastructure.sections.layers')} description={t('infrastructure.layersDescription')} /><div className="grid lg:grid-cols-3">{DOMAIN_ORDER.map((domain) => <LayerColumn key={domain} domain={domain} nodes={domains[domain]} selectedId={selectedId} onSelect={onSelect} t={t} />)}</div></section><section className="border border-forensics-border bg-forensics-surface"><SectionHeading icon={<Network size={15} />} title={t('infrastructure.sections.relations')} description={t('infrastructure.relationsDescription')} /><RelationList edges={edges.slice(0, 8)} nodeNames={nodeNames} t={t} /></section></div>;
}

function LayerColumn({ domain, nodes, selectedId, onSelect, t }: { domain: Domain; nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <div className="border-b border-forensics-border p-4 last:border-b-0 lg:border-b-0 lg:border-r lg:last:border-r-0"><div className="flex items-start justify-between gap-3"><div><div className="text-sm text-forensics-text">{domainLabel(domain, t)}</div><div className="mt-1 text-[11px] leading-4 text-forensics-muted">{t(`infrastructure.domainDescriptions.${domain}`)}</div></div><Badge variant="outline">{nodes.length}</Badge></div><div className="mt-4 divide-y divide-forensics-border border-y border-forensics-border">{nodes.slice(0, 6).map((node) => <NodeListItem key={node.id} node={node} selected={selectedId === node.id} onSelect={onSelect} t={t} />)}</div>{nodes.length > 6 ? <div className="mt-3 text-[11px] text-forensics-muted">{t('infrastructure.moreObjects', { count: nodes.length - 6 })}</div> : null}{nodes.length === 0 ? <div className="mt-4 text-xs text-forensics-muted">{t('infrastructure.values.notFound')}</div> : null}</div>;
}

function Topology({ domains, edges, nodeNames, selectedId, onSelect, t }: { domains: Record<Domain, InfrastructureGraphNode[]>; edges: InfrastructureGraphEdge[]; nodeNames: Map<string, string>; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  const visualDomains = {
    ...domains,
    analysis: domains.analysis.slice(0, 48),
  };
  const visualIds = new Set(DOMAIN_ORDER.flatMap((domain) => visualDomains[domain].map((node) => node.id)));
  const visualEdges = edges.filter((edge) => visualIds.has(edge.sourceId) && visualIds.has(edge.targetId));
  const hiddenEvidenceCount = domains.analysis.length - visualDomains.analysis.length;
  return <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]"><section className="overflow-hidden border border-forensics-border bg-forensics-surface p-4"><div className="mb-4 flex items-center gap-2 text-xs text-forensics-muted"><GitBranch size={14} />{t('infrastructure.topologyHint')}</div><TopologyCanvas domains={visualDomains} edges={visualEdges} selectedId={selectedId} onSelect={onSelect} t={t} />{hiddenEvidenceCount > 0 ? <div className="mt-3 border-t border-forensics-border pt-3 text-[11px] text-forensics-muted">{t('infrastructure.topologyHiddenEvidence', { count: hiddenEvidenceCount })}</div> : null}</section><RelationList edges={edges} nodeNames={nodeNames} t={t} /></div>;
}

function TopologyCanvas({ domains, edges, selectedId, onSelect, t }: { domains: Record<Domain, InfrastructureGraphNode[]>; edges: InfrastructureGraphEdge[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 920, height: 560 });
  const positions = useMemo(() => layoutCanvasNodes(domains, size.width, size.height), [domains, size]);
  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const update = () => setSize({ width: Math.max(760, element.clientWidth), height: Math.max(480, Math.max(...DOMAIN_ORDER.map((domain) => domains[domain].length), 1) * 72 + 86) });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [domains]);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    const context = canvas.getContext('2d');
    if (!context) return;
    context.scale(ratio, ratio);
    drawTopology(context, size, domains, edges, positions, selectedId, t);
  }, [domains, edges, positions, selectedId, size, t]);
  const handleClick = (event: MouseEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / rect.width) * size.width;
    const y = ((event.clientY - rect.top) / rect.height) * size.height;
    const hit = [...positions.entries()].find(([, point]) => x >= point.x - point.width / 2 && x <= point.x + point.width / 2 && y >= point.y - point.height / 2 && y <= point.y + point.height / 2);
    onSelect(hit?.[0] ?? '');
  };
  return <div ref={containerRef} className="w-full overflow-x-auto"><canvas ref={canvasRef} className="block h-auto min-w-[760px] cursor-pointer" onClick={handleClick} role="img" aria-label={t('infrastructure.topologyCanvasLabel')} /></div>;
}

type CanvasPoint = { x: number; y: number; width: number; height: number; domain: Domain };

function layoutCanvasNodes(domains: Record<Domain, InfrastructureGraphNode[]>, width: number, _height: number) {
  const points = new Map<string, CanvasPoint>();
  const laneWidth = width / 3;
  DOMAIN_ORDER.forEach((domain, lane) => {
    domains[domain].forEach((node, index) => {
      points.set(node.id, { x: lane * laneWidth + laneWidth / 2, y: 62 + index * 72, width: laneWidth - 28, height: 48, domain });
    });
  });
  return points;
}

function drawTopology(context: CanvasRenderingContext2D, size: { width: number; height: number }, domains: Record<Domain, InfrastructureGraphNode[]>, edges: InfrastructureGraphEdge[], positions: Map<string, CanvasPoint>, selectedId: string | undefined, t: ReturnType<typeof useTranslation>['t']) {
  const colors = { text: '#1f2933', muted: '#68747c', border: '#c8ced1', lane: '#f4f6f7', selected: '#2d6cdf', edge: '#9ca8ad', environment: '#dcecff', storage: '#e9e1ff', analysis: '#e2f3ea' };
  context.clearRect(0, 0, size.width, size.height);
  context.fillStyle = '#fbfcfc';
  context.fillRect(0, 0, size.width, size.height);
  const laneWidth = size.width / 3;
  DOMAIN_ORDER.forEach((domain, lane) => {
    context.fillStyle = colors.lane;
    context.fillRect(lane * laneWidth + 4, 4, laneWidth - 8, size.height - 8);
    context.fillStyle = colors.text;
    context.font = '13px sans-serif';
    context.fillText(domainLabel(domain, t), lane * laneWidth + 16, 25);
    context.fillStyle = colors.muted;
    context.font = '10px sans-serif';
    context.fillText(`${domains[domain].length}`, (lane + 1) * laneWidth - 28, 25);
  });
  edges.forEach((edge) => {
    const source = positions.get(edge.sourceId);
    const target = positions.get(edge.targetId);
    if (!source || !target || source.domain === target.domain) return;
    context.strokeStyle = colors.edge;
    context.lineWidth = 1.4;
    context.beginPath();
    const forward = DOMAIN_ORDER.indexOf(source.domain) < DOMAIN_ORDER.indexOf(target.domain);
    const startX = forward ? source.x + source.width / 2 : source.x - source.width / 2;
    const endX = forward ? target.x - target.width / 2 : target.x + target.width / 2;
    context.moveTo(startX, source.y);
    context.bezierCurveTo((startX + endX) / 2, source.y, (startX + endX) / 2, target.y, endX, target.y);
    context.stroke();
    drawArrow(context, endX, target.y, colors.edge, forward ? 1 : -1);
  });
  DOMAIN_ORDER.forEach((domain) => domains[domain].forEach((node) => {
    const point = positions.get(node.id);
    if (!point) return;
    const selected = selectedId === node.id;
    const fill = domain === 'environment' ? colors.environment : domain === 'storage' ? colors.storage : colors.analysis;
    context.fillStyle = fill;
    context.strokeStyle = selected ? colors.selected : colors.border;
    context.lineWidth = selected ? 3 : 1;
    context.beginPath();
    context.roundRect(point.x - point.width / 2, point.y - point.height / 2, point.width, point.height, 5);
    context.fill();
    context.stroke();
    context.fillStyle = selected ? colors.selected : colors.muted;
    context.beginPath();
    context.arc(point.x - point.width / 2 + 13, point.y, 5, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = colors.text;
    context.font = selected ? '600 11px sans-serif' : '11px sans-serif';
    context.fillText(trimCanvasLabel(node.name, point.width - 34), point.x - point.width / 2 + 25, point.y - 2);
    context.fillStyle = colors.muted;
    context.font = '9px sans-serif';
    context.fillText(kindLabel(node.kind, t), point.x - point.width / 2 + 25, point.y + 13);
  }));
}

function drawArrow(context: CanvasRenderingContext2D, x: number, y: number, color: string, direction: 1 | -1) {
  context.fillStyle = color;
  context.beginPath();
  context.moveTo(x, y);
  context.lineTo(x - direction * 7, y - 4);
  context.lineTo(x - direction * 7, y + 4);
  context.closePath();
  context.fill();
}

function trimCanvasLabel(value: string, maxWidth: number) {
  if (value.length * 6 <= maxWidth) return value;
  return `${value.slice(0, Math.max(1, Math.floor(maxWidth / 6) - 1))}…`;
}

function Inventory({ nodes, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) {
  return <section className="overflow-hidden border border-forensics-border bg-forensics-surface"><SectionHeading icon={<HardDrive size={15} />} title={t('infrastructure.sections.inventory')} description={t('infrastructure.inventoryDescription', { count: nodes.length })} /><div className="overflow-auto"><Table className="min-w-[700px] text-left text-xs"><TableHeader><TableRow><TableHead>{t('infrastructure.columns.name')}</TableHead><TableHead>{t('infrastructure.columns.layer')}</TableHead><TableHead>{t('infrastructure.columns.type')}</TableHead><TableHead>{t('infrastructure.columns.status')}</TableHead><TableHead>{t('infrastructure.columns.confidence')}</TableHead></TableRow></TableHeader><TableBody>{nodes.map((node) => <TableRow key={node.id} data-state={selectedId === node.id ? 'selected' : undefined} onClick={() => onSelect(node.id)} className="cursor-pointer"><TableCell className="max-w-[280px] truncate text-forensics-text">{node.name}</TableCell><TableCell className="text-forensics-muted">{domainLabel(node.domain, t)}</TableCell><TableCell className="text-forensics-muted">{kindLabel(node.kind, t)}</TableCell><TableCell><State value={node.status} t={t} /></TableCell><TableCell><State value={node.confidence} t={t} /></TableCell></TableRow>)}</TableBody></Table></div>{nodes.length === 0 ? <EmptyState text={t('infrastructure.values.notFound')} compact /> : null}</section>;
}

function RelationList({ edges, nodeNames, t }: { edges: InfrastructureGraphEdge[]; nodeNames: Map<string, string>; t: ReturnType<typeof useTranslation>['t'] }) {
  return <section className="overflow-hidden border border-forensics-border bg-forensics-surface"><SectionHeading icon={<Network size={15} />} title={t('infrastructure.sections.relations')} description={t('infrastructure.relationsDescription')} /><div className="max-h-[540px] divide-y divide-forensics-border overflow-auto">{edges.map((edge, index) => <div key={`${edge.sourceId}-${edge.targetId}-${index}`} className="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-center"><div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.sourceId) ?? edge.sourceId}>{nodeNames.get(edge.sourceId) ?? t('infrastructure.values.unknown')}</div><div className="flex items-center gap-1 text-[10px] text-forensics-muted"><ChevronRight size={13} /><span>{relationLabel(edge.relationKind, t)}</span></div><div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.targetId) ?? edge.targetId}>{nodeNames.get(edge.targetId) ?? t('infrastructure.values.unknown')}</div></div>)}</div>{edges.length === 0 ? <EmptyState text={t('infrastructure.values.notFound')} compact /> : null}</section>;
}

function Inspector({ node, edges, onOpenFiles, onClose, t }: { node: InfrastructureGraphNode; edges: InfrastructureGraphEdge[]; onOpenFiles: () => void; onClose: () => void; t: ReturnType<typeof useTranslation>['t'] }) {
  const provenance = parseJson(node.provenanceJson);
  const linked = edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id);
  const sourceId = typeof provenance.dataSourceId === 'string' ? provenance.dataSourceId : undefined;
  return <aside className="shrink-0 border-t border-forensics-border bg-forensics-surface px-5 py-4 lg:px-7"><div className="mx-auto flex max-w-[1500px] flex-col gap-4 lg:flex-row lg:items-start lg:justify-between"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="text-sm text-forensics-text">{node.name}</span><State value={node.status} t={t} /></div><div className="mt-1 text-xs text-forensics-muted">{domainLabel(node.domain, t)} · {kindLabel(node.kind, t)} · {t('infrastructure.inspector.linkedRelations', { count: linked.length })}</div><div className="mt-3 grid gap-2 text-xs sm:grid-cols-3"><Detail label={t('infrastructure.columns.confidence')} value={stateLabel(node.confidence, t)} /><Detail label={t('infrastructure.inspector.source')} value={sourceId ?? t('infrastructure.values.unavailable')} /><Detail label={t('infrastructure.inspector.parser')} value={typeof provenance.parser === 'string' ? provenance.parser : t('infrastructure.values.unavailable')} /></div></div><div className="flex shrink-0 items-center gap-2"><Button size="xs" variant="forensicsOutline" onClick={() => navigator.clipboard?.writeText(node.id)} title={t('infrastructure.actions.copyId')}><Copy size={12} />{t('infrastructure.actions.copyId')}</Button>{sourceId ? <Button size="xs" variant="forensicsOutline" onClick={onOpenFiles}><ExternalLink size={12} />{t('infrastructure.actions.openFiles')}</Button> : null}<Button size="xs" variant="forensicsGhost" onClick={onClose}>{t('infrastructure.actions.close')}</Button></div></div></aside>;
}

function NodeListItem({ node, selected, onSelect, t }: { node: InfrastructureGraphNode; selected: boolean; onSelect: (id: string) => void; t: ReturnType<typeof useTranslation>['t'] }) { return <Button size="inline" variant="forensicsGhost" onClick={() => onSelect(node.id)} className={`flex w-full items-center justify-between gap-3 border-l-2 py-2 text-left hover:text-forensics-primary-blue ${selected ? 'border-forensics-primary-blue bg-forensics-hover' : 'border-transparent'}`}><span className="min-w-0 truncate text-xs text-forensics-text">{node.name}</span><span className="shrink-0 text-[10px] text-forensics-muted">{kindLabel(node.kind, t)}</span></Button>; }
function SectionHeading({ icon, title, description }: { icon: ReactNode; title: string; description: string }) { return <div className="border-b border-forensics-border px-4 py-3"><div className="flex items-center gap-2 text-sm text-forensics-text"><span className="text-forensics-muted">{icon}</span>{title}</div><div className="mt-1 text-[11px] text-forensics-muted">{description}</div></div>; }
function Detail({ label, value }: { label: string; value: string }) { return <div className="min-w-0 border-l border-forensics-border pl-2"><div className="text-[10px] uppercase tracking-wide text-forensics-muted">{label}</div><div className="mt-1 truncate text-xs text-forensics-text" title={value}>{value}</div></div>; }
function Metric({ icon, label, value }: { icon: ReactNode; label: string; value: string | number }) { return <div className="min-w-0 border-r border-forensics-border px-3 py-3 last:border-r-0"><div className="flex items-center gap-2 text-[10px] uppercase tracking-wide text-forensics-muted">{icon}{label}</div><div className="mt-1 font-mono text-lg text-forensics-text">{value}</div></div>; }
function State({ value, t }: { value: string; t: ReturnType<typeof useTranslation>['t'] }) { return <Badge variant={value === 'ready' || value === 'complete' || value === 'verified' ? 'default' : 'outline'}>{stateLabel(value, t)}</Badge>; }
function EmptyState({ text, compact }: { text: string; compact?: boolean }) { return <div className={`${compact ? 'p-6' : 'flex flex-1 items-center justify-center'} text-sm text-forensics-muted`}>{text}</div>; }

function groupByDomain(nodes: InfrastructureGraphNode[]) { return Object.fromEntries(DOMAIN_ORDER.map((domain) => [domain, nodes.filter((node) => node.domain === domain)])) as Record<Domain, InfrastructureGraphNode[]>; }
function matchesQuery(node: InfrastructureGraphNode, query: string, t: ReturnType<typeof useTranslation>['t']) { const normalized = query.trim().toLocaleLowerCase(); return !normalized || [node.name, node.id, domainLabel(node.domain, t), kindLabel(node.kind, t)].some((value) => value.toLocaleLowerCase().includes(normalized)); }
function parseJson(value: string) { try { return JSON.parse(value) as Record<string, unknown>; } catch { return {}; } }
function domainLabel(domain: Domain, t: ReturnType<typeof useTranslation>['t']) { return t(`infrastructure.domains.${domain}`); }
function kindLabel(kind: string, t: ReturnType<typeof useTranslation>['t']) { const key = `infrastructure.kinds.${kind}`; const translated = t(key); return translated === key ? humanize(kind) : translated; }
function relationLabel(relation: string, t: ReturnType<typeof useTranslation>['t']) { const key = `infrastructure.relations.${relation}`; const translated = t(key); return translated === key ? humanize(relation) : translated; }
function stateLabel(value: string, t: ReturnType<typeof useTranslation>['t']) { const key = `infrastructure.states.${value}`; const translated = t(key); return translated === key ? humanize(value) : translated; }
function humanize(value: string) { return value.replace(/_/g, ' ').replace(/\b\w/g, (letter: string) => letter.toUpperCase()); }
