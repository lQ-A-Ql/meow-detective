import { ChevronRight, Clock, ZoomIn, ZoomOut } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { TimelineHistogram } from '@/features/timeline/components/TimelineHistogram';
import type { TimelineWorkspaceModel } from '@/features/timeline/use-timeline-workspace-model';
import type { TimelineEvent } from '@/types/models';

const TIMELINE_COLUMNS: DenseColumn<TimelineEvent>[] = [
  { key: 'ts', title: '时间戳', className: 'w-36 text-forensics-muted', render: (row) => row.ts },
  { key: 'source', title: '数据源', className: 'w-28 text-forensics-muted-light', render: (row) => row.dataSourceId ?? '-' },
  { key: 'eventType', title: '类型', className: 'w-28 text-forensics-muted', render: (row) => row.eventType },
  { key: 'title', title: '描述', className: 'text-forensics-text-secondary', render: (row) => row.title },
];

interface TimelineWorkspaceProps {
  model: TimelineWorkspaceModel;
}

/** Pure timeline presentation surface. Query and selection behavior belong to the workspace model. */
export function TimelineWorkspace({ model }: TimelineWorkspaceProps) {
  return (
    <div className="flex h-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar title="时间线控制带" meta={`事件 ${model.events.length}/${model.totalEvents} 条 / 数据源 ${model.sourceCount} 个`}>
        <div className="flex min-h-10 shrink-0 items-center justify-between gap-3 overflow-x-auto px-4 py-1">
          <div className="flex items-center gap-4 whitespace-nowrap">
            <div className="flex items-center gap-2 font-mono text-[11px] text-forensics-muted">
              <Clock size={12} className="text-forensics-muted-light" />
              <span className="text-forensics-text">{model.timeRange.start}</span>
              <span className="text-forensics-500">至</span>
              <span className="text-forensics-text">{model.timeRange.end}</span>
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <div className="flex items-center gap-2 text-[11px] text-forensics-muted-light">
              粒度:
              <span className="border border-forensics-border-strong bg-forensics-surface px-1.5 py-0.5 text-forensics-text">自适应</span>
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              起始
              <Input type="datetime-local" step={1} value={model.draftTimeStart} onChange={(event) => model.setDraftTimeStart(event.target.value)} variant="mono" inputSize="inline" className={model.draftDatesValid ? '' : 'border-forensics-error-border'} />
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              结束
              <Input type="datetime-local" step={1} value={model.draftTimeEnd} onChange={(event) => model.setDraftTimeEnd(event.target.value)} variant="mono" inputSize="inline" className={model.draftDatesValid ? '' : 'border-forensics-error-border'} />
            </label>
            {!model.draftDatesValid ? <span className="text-[11px] text-forensics-error-text">日期无效</span> : null}
            <Button type="button" variant="forensicsOutline" size="compact" onClick={model.applyDateRange} disabled={!model.draftDatesValid}>应用</Button>
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              类型
              <Select value={model.eventType} onValueChange={model.selectEventType}>
                <SelectTrigger variant="forensics" size="xs" className="w-28"><SelectValue placeholder="全部" /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="__all__">全部</SelectItem>
                  {model.eventTypes.map((type) => <SelectItem key={type} value={type}>{type}</SelectItem>)}
                </SelectContent>
              </Select>
            </label>
            <Button type="button" variant="forensicsOutline" size="compact" onClick={model.clearFilters}>清除</Button>
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="forensicsGhost" size="iconSm" onClick={model.zoomOut} disabled={!model.canZoomOut} aria-label="缩小"><ZoomOut size={14} /></Button>
            <Button type="button" variant="forensicsGhost" size="iconSm" onClick={model.zoomIn} disabled={!model.canZoomIn} aria-label="放大"><ZoomIn size={14} /></Button>
          </div>
        </div>
      </PageSubbar>

      <div className="flex min-h-24 shrink-0 flex-col border-b border-forensics-border bg-forensics-panel p-2">
        <TimelineHistogram bars={model.bars} onSelectRange={model.selectTimeBucket} />
        <div className="mt-1 flex justify-between px-2 pt-1 font-mono text-[9px] text-forensics-muted-light">
          <span>{model.timeRange.start}</span>
          <span className="font-light text-forensics-text-secondary">{model.middleTimestamp}</span>
          <span>{model.timeRange.end}</span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div
          data-testid="timeline-table-pane"
          className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden border-r border-forensics-border"
        >
          <DenseDataTable<TimelineEvent>
            rows={model.tableEvents}
            getRowKey={(row) => row.id}
            selectedRowKey={model.selectedEvent?.id}
            onRowClick={model.onEventRowClick}
            emptyTitle="当前时间范围无事件"
            emptyDescription="请扩大时间范围或调整事件过滤条件。"
            columns={TIMELINE_COLUMNS}
            loadContextKey={model.loadContextKey}
            loadStateKey={model.loadStateKey}
            onReachEnd={model.loadNextPage}
            onRetryLoadMore={model.retry}
            hasMore={model.hasMore}
            loadingMore={model.loadingMore}
            loadMoreFailed={model.loadMoreFailed}
            initialLoadFailed={model.initialLoadFailed}
            onRetryInitialLoad={model.retry}
          />
        </div>
        <InspectorPane title="事件检查器" subtitle={model.selectedEvent ? `当前事件 ${model.selectedEvent.id}` : '未选择事件'} widthClassName="w-80">
          <div className="space-y-5">
            <InspectorSection title="时间戳"><InspectorValue value={model.selectedEvent?.ts ?? '-'} mono strong /></InspectorSection>
            <InspectorSection title="事件类型"><InspectorValue value={model.selectedEvent?.eventType ?? '-'} /></InspectorSection>
            <InspectorSection title="源活动"><InspectorValue value={model.selectedEvent?.description ?? '-'} /></InspectorSection>
            <InspectorSection title="来源对象"><InspectorValue value={model.selectedEvent?.title ?? '-'} mono /></InspectorSection>
            <InspectorSection title="时间上下文"><div className="space-y-1 font-mono text-[10px] text-forensics-muted"><div className="max-w-full truncate">source: {model.selectedEvent?.dataSourceId ?? '-'}</div><div className="max-w-full truncate">window: {model.selectedEvent?.ts ?? '-'} +/- 10m</div></div></InspectorSection>
            <InspectorSection title="关联动作"><Button type="button" variant="forensicsSurface" size="xs" onClick={() => model.jumpToSource(model.selectedEvent)} disabled={!model.selectedEvent} className="w-full justify-between font-mono text-forensics-text-tertiary"><span className="font-light">跳转到来源对象</span><ChevronRight size={12} className="text-forensics-muted-light" /></Button></InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
