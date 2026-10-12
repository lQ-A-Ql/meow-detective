import { Clock, ZoomIn, ZoomOut } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
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
import { ContextBar } from '@/components/layout/ContextBar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { TimelineHistogram } from '@/features/timeline/components/TimelineHistogram';
import { TimelineRangeSelector } from '@/features/timeline/components/TimelineRangeSelector';
import type { TimelineWorkspaceModel } from '@/features/timeline/use-timeline-workspace-model';
import type { TimelineEvent } from '@/types/models';
import { WorkspaceFrame } from '@/components/layout/WorkspaceFrame';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';

interface TimelineWorkspaceProps {
  model: TimelineWorkspaceModel;
}

/** Pure timeline presentation surface. Query and selection behavior belong to the workspace model. */
export function TimelineWorkspace({ model }: TimelineWorkspaceProps) {
  const { t } = useTranslation();
  const columns: DenseColumn<TimelineEvent>[] = [
    { key: 'ts', title: t('timeline.columns.timestamp'), className: 'w-36 text-forensics-muted', render: (row) => row.ts },
    { key: 'source', title: t('timeline.columns.source'), className: 'w-28 text-forensics-muted-light', render: (row) => row.dataSourceId ?? '-' },
    { key: 'eventType', title: t('timeline.columns.type'), className: 'w-28 text-forensics-muted', render: (row) => row.eventType },
    { key: 'title', title: t('timeline.columns.description'), className: 'text-forensics-text-secondary', render: (row) => row.title },
  ];
  return (
    <WorkspaceFrame>
      <WorkspaceCard className="flex flex-1 flex-col">
      <ContextBar>
        <div className="flex min-w-0 flex-1 items-center justify-between gap-3 overflow-x-auto scrollbar-none">
          <div className="flex items-center gap-4 whitespace-nowrap">
            <div className="flex items-center gap-2 font-mono text-[11px] text-forensics-muted">
              <Clock size={12} className="text-forensics-muted-light" />
              <span className="text-forensics-text">{model.timeRange.start}</span>
              <span className="text-forensics-500">{t('timeline.rangeTo')}</span>
              <span className="text-forensics-text">{model.timeRange.end}</span>
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <div className="flex items-center gap-2 text-[11px] text-forensics-muted-light">
              {t('timeline.granularity')}: <span className="border border-forensics-border-strong bg-forensics-surface px-1.5 py-0.5 text-forensics-text">{t('timeline.adaptive')}</span>
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              {t('timeline.start')}
              <Input type="datetime-local" step={1} value={model.draftTimeStart} onChange={(event) => model.setDraftTimeStart(event.target.value)} variant="mono" inputSize="inline" className={model.draftDatesValid ? '' : 'border-forensics-error-border'} />
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              {t('timeline.end')}
              <Input type="datetime-local" step={1} value={model.draftTimeEnd} onChange={(event) => model.setDraftTimeEnd(event.target.value)} variant="mono" inputSize="inline" className={model.draftDatesValid ? '' : 'border-forensics-error-border'} />
            </label>
            {!model.draftDatesValid ? <span className="text-[11px] text-forensics-error-text">{t('timeline.invalidDate')}</span> : null}
            <Tooltip content={t('timeline.apply')}><Button aria-label={t('timeline.apply')} type="button" variant="forensicsOutline" size="compact" onClick={model.applyDateRange} disabled={!model.draftDatesValid}><ActionIcon name="history" /></Button></Tooltip>
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              {t('timeline.type')}
              <Select value={model.eventType} onValueChange={model.selectEventType}>
                <SelectTrigger variant="forensics" size="xs" className="w-28"><SelectValue placeholder={t('timeline.all')} /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="__all__">{t('timeline.all')}</SelectItem>
                  {model.eventTypes.map((type) => <SelectItem key={type} value={type}>{type}</SelectItem>)}
                </SelectContent>
              </Select>
            </label>
            <Tooltip content={t('timeline.clear')}><Button aria-label={t('timeline.clear')} type="button" variant="forensicsOutline" size="compact" onClick={model.clearFilters}><ActionIcon name="history" /></Button></Tooltip>
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="forensicsGhost" size="iconSm" onClick={model.zoomOut} disabled={!model.canZoomOut} aria-label={t('graph.controls.zoomOut')}><ZoomOut size={14} /></Button>
            <Button type="button" variant="forensicsGhost" size="iconSm" onClick={model.zoomIn} disabled={!model.canZoomIn} aria-label={t('graph.controls.zoomIn')}><ZoomIn size={14} /></Button>
          </div>
        </div>
      </ContextBar>

      <div className="flex h-32 shrink-0 flex-col p-4">
        <TimelineHistogram bars={model.bars} onSelectRange={model.selectTimeBucket} />
        <TimelineRangeSelector
          domainStart={model.timelineStartTs}
          domainEnd={model.timelineEndTs}
          selectedStart={model.selectedTimeStart}
          selectedEnd={model.selectedTimeEnd}
          onChange={model.selectTimeRange}
          startLabel={t('timeline.rangeSelector.start')}
          endLabel={t('timeline.rangeSelector.end')}
          rangeLabel={t('timeline.rangeSelector.label')}
        />
      </div>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div
          data-testid="timeline-table-pane"
          className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden border-r border-forensics-border"
        >
          <DenseDataTableFrame layout="fill" variant="plain">
            <DenseDataTable<TimelineEvent>
            rows={model.tableEvents}
            getRowKey={(row) => row.id}
            selectedRowKey={model.selectedEvent?.id}
            onRowClick={model.onEventRowClick}
            emptyTitle={t('timeline.emptyTitle')}
            emptyAction={(
              <Tooltip content={t('timeline.clear')}><Button aria-label={t('timeline.clear')} type="button" variant="outline" size="sm" onClick={model.clearFilters}><ActionIcon name="history" /></Button></Tooltip>
            )}
            emptyDescription={t('timeline.emptyDescription')}
            columns={columns}
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
          </DenseDataTableFrame>
        </div>
        <InspectorPane kind={t('inspectorKind.timelineEvent')} title={t('timeline.inspector.title')} subtitle={model.selectedEvent ? `${t('timeline.inspector.current')} ${model.selectedEvent.id}` : t('timeline.inspector.none')} widthClassName="w-80">
          <div className="space-y-5">
            <InspectorSection title={t('timeline.inspector.timestamp')}><InspectorValue value={model.selectedEvent?.ts ?? '-'} mono strong /></InspectorSection>
            <InspectorSection title={t('timeline.inspector.type')}><InspectorValue value={model.selectedEvent?.eventType ?? '-'} /></InspectorSection>
            <InspectorSection title={t('timeline.inspector.activity')}><InspectorValue value={model.selectedEvent?.description ?? '-'} /></InspectorSection>
            <InspectorSection title={t('timeline.inspector.object')}><InspectorValue value={model.selectedEvent?.title ?? '-'} mono /></InspectorSection>
            <InspectorSection title={t('timeline.inspector.context')}><div className="space-y-1 font-mono text-[10px] text-forensics-muted"><div className="max-w-full truncate">source: {model.selectedEvent?.dataSourceId ?? '-'}</div><div className="max-w-full truncate">window: {model.selectedEvent?.ts ?? '-'} +/- 10m</div></div></InspectorSection>
            <InspectorSection title={t('timeline.inspector.actions')}><Tooltip content={t('timeline.jumpToSource')}><Button type="button" variant="forensicsSurface" size="xs" onClick={() => model.jumpToSource(model.selectedEvent)} disabled={!model.selectedEvent} aria-label={t('timeline.jumpToSource')}><ActionIcon name="chevron-right" /></Button></Tooltip></InspectorSection>
          </div>
        </InspectorPane>
      </div>
      </WorkspaceCard>
    </WorkspaceFrame>
  );
}
