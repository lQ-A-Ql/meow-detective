import { useTranslation } from 'react-i18next';
import { Boxes, CircleDot, Database, GitBranch, HardDrive, Search } from 'lucide-react';
import { useNavigate } from 'react-router';
import { Input } from '@/app/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { EmptyState } from '@/components/data-display';
import { useInfrastructureWorkspace } from '../hooks/useInfrastructureWorkspace';
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
  const workspace = useInfrastructureWorkspace();
  const { currentCase, graph, nodes, edges, visibleNodes, nodeNames, networkTopology, hostFacts, query, selected, selectedId, setQuery, setSelectedId } = workspace;

  if (!currentCase.data) return <EmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.noCase')}</EmptyState>;
  if (graph.isLoading) return <EmptyState className="flex flex-1 items-center justify-center">{t('common.loading')}</EmptyState>;
  if (!graph.data || nodes.length === 0) return <EmptyState className="flex flex-1 items-center justify-center">{t('infrastructure.empty')}</EmptyState>;

  const workloads = nodes.filter((node) => node.domain === 'analysis');
  const storage = nodes.filter((node) => node.domain === 'storage');
  return (
    <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-forensics-panel">
      <InfrastructureHeader caseName={currentCase.data.name} nodes={nodes} relationCount={edges.length} networkLinkCount={networkTopology.edges.length} t={t} />
      <Tabs defaultValue="overview" className="min-h-0 flex-1 overflow-hidden px-5 py-4 lg:px-7">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-forensics-border">
          <TabsList className="max-w-full overflow-x-auto scrollbar-none">
            <TabsTrigger value="overview"><CircleDot size={14} />{t('infrastructure.tabs.overview')}</TabsTrigger>
            <TabsTrigger value="topology"><GitBranch size={14} />{t('infrastructure.tabs.topology')}</TabsTrigger>
            <TabsTrigger value="workloads"><Boxes size={14} />{t('infrastructure.tabs.workloads')}</TabsTrigger>
            <TabsTrigger value="storage"><Database size={14} />{t('infrastructure.tabs.storage')}</TabsTrigger>
            <TabsTrigger value="inventory"><HardDrive size={14} />{t('infrastructure.tabs.inventory')}</TabsTrigger>
          </TabsList>
          <div className="relative mb-1 w-full sm:w-64"><Search className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-forensics-muted" size={14} /><Input aria-label={t('infrastructure.actions.search')} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t('infrastructure.searchPlaceholder')} variant="forensics" inputSize="compact" className="pl-7" /></div>
        </div>
        <TabsContent value="overview" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><ClusterOverviewPanel nodes={nodes} edges={edges} hostFacts={hostFacts} t={t} /></TabsContent>
        <TabsContent value="topology" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><NetworkTopologyPanel nodes={networkTopology.nodes} edges={networkTopology.edges} nodeNames={nodeNames} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="workloads" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><WorkloadEvidencePanel nodes={workloads} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="storage" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><StorageEvidencePanel nodes={storage} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
        <TabsContent value="inventory" className="h-[calc(100%-46px)] overflow-auto scrollbar-none pt-4"><InfrastructureInventoryPanel nodes={visibleNodes} selectedId={selectedId} onSelect={setSelectedId} t={t} /></TabsContent>
      </Tabs>
      {selected ? <InfrastructureInspector node={selected} edges={edges} onOpenFiles={() => navigate('/files')} onClose={() => setSelectedId(undefined)} t={t} /> : null}
    </main>
  );
}
