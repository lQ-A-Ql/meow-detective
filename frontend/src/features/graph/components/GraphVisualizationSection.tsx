import { RefreshCw, CircleDot } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';

import { ColorSwatch } from '@/components/data-display';
import { ForceGraph } from '@/features/graph/visualization/ForceGraph';
import { GraphEdgeDetails } from '@/features/graph/components/GraphEdgeDetails';
import { GraphNodeDetails } from '@/features/graph/components/GraphNodeDetails';
import { ALL_EDGE_TYPES, edgeTypeColor, EDGE_TYPE_KEYS } from '@/features/graph/logic/graph-utils';
import type { GraphVisualizationModel } from '@/features/graph/use-graph-visualization-model';

export function GraphVisualizationSection({ model }: { model: GraphVisualizationModel }) {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLDivElement>(null);
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    if (!canvasRef.current) return;
    const element = canvasRef.current;
    const observer = new ResizeObserver(([entry]) => {
      setCanvasSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <section>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-2 text-[11px]">
            <span className="text-forensics-muted">{t('graph.depth')}</span>
            <Select value={String(model.maxDepth)} onValueChange={(value) => model.setMaxDepth(Number(value))}>
              <SelectTrigger size="xs" variant="forensics" className="w-16"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="1">1</SelectItem>
                <SelectItem value="2">2</SelectItem>
                <SelectItem value="3">3</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Tooltip content={t('graph.pause')}><Button aria-label={t('graph.pause')}
            type="button"
            variant="outline"
            onClick={model.toggleRunning}
            disabled={!model.hasNodes}
            className="rounded-none border-forensics-border bg-forensics-surface text-[11px] hover:bg-forensics-panel-strong"
          ><ActionIcon name="pause" /></Button></Tooltip>
          <Tooltip content={t('graph.refresh')}><Button aria-label={t('graph.refresh')}
            type="button"
            variant="outline"
            onClick={model.refresh}
            disabled={model.isLoadingGraph}
            className="rounded-none border-forensics-border bg-forensics-surface text-[11px] hover:bg-forensics-panel-strong"
          ><ActionIcon name="refresh" /></Button></Tooltip>
        </div>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <span className="text-[11px] text-forensics-muted">{t('graph.edgeTypeLabel')}</span>
        <Tooltip content={t('graph.selectAll') + ' / ' + t('graph.clearAll')}><Button aria-label={t('graph.selectAll') + ' / ' + t('graph.clearAll')}
          type="button"
          variant="forensicsGhost"
          size="compact"
          onClick={() => model.selectAllEdgeTypes(model.selectedEdgeTypes.length < ALL_EDGE_TYPES.length)}
          className="text-[10px]"
        ><CircleDot size={15} /></Button></Tooltip>
        {ALL_EDGE_TYPES.map((type) => (
          <label key={type} className="flex cursor-pointer items-center gap-1 rounded-none bg-transparent px-2 py-1 text-[10px] hover:bg-forensics-panel-strong">
            <Checkbox
              checked={model.selectedEdgeTypes.includes(type)}
              onCheckedChange={() => model.toggleEdgeType(type)}
              variant="forensics"
              checkboxSize="compact"
            />
            <ColorSwatch color={edgeTypeColor(type)} />
            <span>{t(EDGE_TYPE_KEYS[type])}</span>
          </label>
        ))}
      </div>

      <div className="mt-3 flex h-[420px] overflow-hidden rounded-none bg-transparent">
        <div ref={canvasRef} className="relative flex-1">
          {!model.hasNodes && !model.isLoadingGraph ? (
            <div className="flex h-full flex-col items-center justify-center p-6 text-center text-[12px] text-forensics-muted">
              <div>{t('graph.empty.title')}</div>
              <div className="mt-1">{t('graph.empty.description')}</div>
            </div>
          ) : (
            <ForceGraph
              nodes={model.graphData.nodes}
              edges={model.graphData.edges}
              selectedNodeId={model.selectedNodeId}
              selectedEdgeId={model.selectedEdgeId}
              onNodeClick={(node) => model.selectNode(node.id)}
              onNodeDoubleClick={(node) => model.expandNode(node.id, 1)}
              onEdgeClick={(edge) => model.selectEdge(edge.id)}
              onBackgroundClick={model.clearSelection}
              width={canvasSize.width}
              height={canvasSize.height}
              running={model.running}
            />
          )}
          {model.isLoadingGraph ? (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-forensics-surface/60">
              <div className="flex items-center gap-2 rounded-none bg-transparent px-3 py-2 text-[11px]">
                <RefreshCw size={12} className="opacity-70" />{t('graph.loading')}
              </div>
            </div>
          ) : null}
        </div>

        <ScrollArea className="min-h-0 w-64 shrink-0 border-l border-forensics-divider bg-forensics-surface" viewportClassName="p-3">
          {model.selectedNode ? (
            <GraphNodeDetails
              node={model.selectedNode}
              edges={model.graphData.edges}
              onExpand={(depth) => model.expandNode(model.selectedNode!.id, depth)}
              onClose={() => model.selectNode()}
            />
          ) : model.selectedEdge ? (
            <GraphEdgeDetails
              edge={model.selectedEdge}
              nodeMap={model.nodeMap}
              provenance={model.provenance}
              provenanceLoading={model.provenanceLoading}
              onClose={() => model.selectEdge()}
            />
          ) : (
            <GraphSummary model={model} />
          )}
        </ScrollArea>
      </div>
    </section>
  );
}

function GraphSummary({ model }: { model: GraphVisualizationModel }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3 text-[11px] text-forensics-muted">
      <p>{t('graph.help.selection')}</p>
      <p>{t('graph.help.navigation')}</p>
      {model.snapshot ? (
        <div className="space-y-1 rounded-none bg-transparent p-2">
          {[
            [t('graph.stats.dataSources'), model.snapshot.dataSourceCount],
            [t('graph.stats.crossSourceEntities'), model.snapshot.crossSourceEntityCount],
            [t('graph.stats.crossSourceRelations'), model.snapshot.crossSourceEdgeCount],
            [t('graph.stats.nodes'), model.snapshot.totalNodes],
            [t('graph.stats.edges'), model.snapshot.totalEdges],
            [t('graph.stats.density'), model.snapshot.density],
          ].map(([label, value]) => (
            <div key={label} className="flex justify-between">
              <span>{label}</span><span className="font-mono text-forensics-text">{value}</span>
            </div>
          ))}
        </div>
      ) : null}
      {model.truncated ? <p className="text-forensics-warning">{t('graph.truncated')}</p> : null}
    </div>
  );
}
