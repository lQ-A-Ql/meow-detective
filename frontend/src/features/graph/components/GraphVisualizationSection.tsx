import { GitBranch, Pause, Play, RefreshCw } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { SectionHeader } from '@/components/data-display';
import { ForceGraph } from '@/features/graph/components/ForceGraph';
import { GraphEdgeDetails } from '@/features/graph/components/GraphEdgeDetails';
import { GraphNodeDetails } from '@/features/graph/components/GraphNodeDetails';
import { ALL_EDGE_TYPES, edgeTypeColor, EDGE_LABELS } from '@/features/graph/components/graph-utils';
import type { GraphVisualizationModel } from '@/features/graph/use-graph-visualization-model';

export function GraphVisualizationSection({ model }: { model: GraphVisualizationModel }) {
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
        <SectionHeader icon={GitBranch} title="关系图谱" subtitle="案件级跨数据源实体与证据邻域" />
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-2 text-[11px]">
            <span className="text-forensics-muted">深度</span>
            <Select value={String(model.maxDepth)} onValueChange={(value) => model.setMaxDepth(Number(value))}>
              <SelectTrigger size="xs" variant="forensics" className="w-16"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="1">1</SelectItem>
                <SelectItem value="2">2</SelectItem>
                <SelectItem value="3">3</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Button
            type="button"
            variant="outline"
            onClick={model.toggleRunning}
            disabled={!model.hasNodes}
            className="h-7 rounded-none border-forensics-border bg-forensics-surface px-2 text-[11px] hover:bg-forensics-panel-strong"
          >
            {model.running ? <Pause size={12} className="mr-1" /> : <Play size={12} className="mr-1" />}
            {model.running ? '暂停' : '继续'}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={model.refresh}
            disabled={model.isLoadingGraph}
            className="h-7 rounded-none border-forensics-border bg-forensics-surface px-2 text-[11px] hover:bg-forensics-panel-strong"
          >
            <RefreshCw size={12} className="mr-1 opacity-70" />刷新
          </Button>
        </div>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <span className="text-[11px] text-forensics-muted">关系类型:</span>
        <Button
          type="button"
          variant="forensicsGhost"
          size="compact"
          onClick={() => model.selectAllEdgeTypes(model.selectedEdgeTypes.length < ALL_EDGE_TYPES.length)}
          className="text-[10px]"
        >
          {model.selectedEdgeTypes.length < ALL_EDGE_TYPES.length ? '全选' : '清空'}
        </Button>
        {ALL_EDGE_TYPES.map((type) => (
          <label key={type} className="flex cursor-pointer items-center gap-1 rounded-none border border-forensics-border bg-forensics-surface px-2 py-1 text-[10px] hover:bg-forensics-panel-strong">
            <Checkbox
              checked={model.selectedEdgeTypes.includes(type)}
              onCheckedChange={() => model.toggleEdgeType(type)}
              variant="forensics"
              checkboxSize="compact"
            />
            <span className="inline-block h-2 w-2 rounded-none" style={{ backgroundColor: edgeTypeColor(type) }} />
            <span>{EDGE_LABELS[type]}</span>
          </label>
        ))}
      </div>

      <div className="mt-3 flex h-[420px] overflow-hidden rounded-none border border-forensics-border bg-forensics-panel">
        <div ref={canvasRef} className="relative flex-1">
          {!model.hasNodes && !model.isLoadingGraph ? (
            <div className="flex h-full flex-col items-center justify-center p-6 text-center text-[12px] text-forensics-muted">
              <div>暂无确定性的跨数据源实体关联。</div>
              <div className="mt-1">完成至少两个数据源的痕迹与实体提取后，将在此处显示案件关系网络。</div>
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
              <div className="flex items-center gap-2 rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] shadow-sm">
                <RefreshCw size={12} className="opacity-70" />加载图数据...
              </div>
            </div>
          ) : null}
        </div>

        <ScrollArea className="min-h-0 w-64 shrink-0 border-l border-forensics-border bg-forensics-surface" viewportClassName="p-3">
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
  return (
    <div className="space-y-3 text-[11px] text-forensics-muted">
      <p>单击节点查看详情，双击节点展开邻域；单击边查看来源追溯。</p>
      <p>拖拽节点调整位置，拖拽空白处平移，滚轮缩放。</p>
      {model.snapshot ? (
        <div className="space-y-1 rounded-none border border-forensics-border bg-forensics-surface p-2">
          {[
            ['数据源', model.snapshot.dataSourceCount],
            ['跨源实体', model.snapshot.crossSourceEntityCount],
            ['跨源关系', model.snapshot.crossSourceEdgeCount],
            ['节点', model.snapshot.totalNodes],
            ['边', model.snapshot.totalEdges],
            ['密度', model.snapshot.density],
          ].map(([label, value]) => (
            <div key={label} className="flex justify-between">
              <span>{label}</span><span className="font-mono text-forensics-text">{value}</span>
            </div>
          ))}
        </div>
      ) : null}
      {model.truncated ? <p className="text-forensics-warning">当前图窗口已达到节点或关系预算，请缩小深度或关系类型。</p> : null}
    </div>
  );
}
