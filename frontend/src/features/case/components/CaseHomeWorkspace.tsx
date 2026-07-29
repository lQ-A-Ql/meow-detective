import { Upload } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { CaseWelcomeForms } from '@/features/case/components/CaseActions';
import {
  CaseMetricsStrip,
  DataSourcesPanel,
  RecentObjectsPanel,
  RecentTasksPanel,
} from '@/features/case/components/CaseOverview';
import type { CaseHomeModel } from '@/features/case/use-case-home-model';
import { ImportDataSourceDialog } from '@/features/import/components/ImportDataSourceDialog';

interface CaseHomeWorkspaceProps {
  model: CaseHomeModel;
}

/** Pure Case Home presentation surface. Case operations belong to the feature model. */
export function CaseHomeWorkspace({ model }: CaseHomeWorkspaceProps) {
  if (!model.currentCase) {
    return (
      <CaseWelcomeForms
        caseRoot={model.caseRoot}
        setCaseRoot={model.updateCaseRoot}
        caseName={model.caseName}
        setCaseName={model.setCaseName}
        onCreateCase={model.createCase}
        createPending={model.createCasePending}
        createError={model.createCaseError}
        openCasePath={model.openCasePath}
        setOpenCasePath={model.setOpenCasePath}
        onOpenCase={model.openCase}
        openPending={model.openCasePending}
        openError={model.openCaseError}
        recentCases={model.recentCases}
        onDeleteCase={model.deleteCase}
      />
    );
  }

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="mb-1 font-serif text-2xl tracking-tight text-forensics-text">案件 #{model.currentCase.number ?? '-'}</div>
            <div className="font-mono text-[11px] text-forensics-muted">{model.currentCase.name}</div>
            <div className="mt-3 flex flex-wrap gap-2 text-[10px] uppercase tracking-wider text-forensics-muted">
              <span className="border border-forensics-border-strong bg-forensics-surface px-2 py-1">当前状态: 活跃</span>
              <span className="border border-forensics-border-strong bg-forensics-surface px-2 py-1">数据源: {model.metrics?.dataSourceCount ?? 0}</span>
              <span className="border border-forensics-warning-border bg-forensics-surface px-2 py-1">告警: {model.warnings?.length ?? 0}</span>
              <span className="border border-forensics-warning-border bg-forensics-surface px-2 py-1">部分完成任务: {model.partialJobCount}</span>
            </div>
          </div>
          <div className="flex gap-8 text-right">
            <div><div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">状态</div><div className="flex items-center justify-end gap-1.5 text-[13px] text-forensics-text"><div className="h-1.5 w-1.5 rounded-none bg-forensics-text" /> 活跃</div></div>
            <div><div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">创建时间</div><div className="font-mono text-[13px] text-forensics-text">{model.currentCase.createdAt}</div></div>
            <div><div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">检验人</div><div className="text-[13px] text-forensics-text">{model.currentCase.examiner ?? '-'}</div></div>
            <Button type="button" variant="forensicsOutline" size="xs" onClick={() => model.setImportDialogOpen(true)}><Upload size={12} /> {model.importButtonLabel}</Button>
          </div>
        </div>
      </div>
      <ImportDataSourceDialog open={model.importDialogOpen} onOpenChange={model.setImportDialogOpen} onImport={model.importDataSource} importPending={model.importPending} />
      <CaseMetricsStrip dataSourceCount={model.metrics?.dataSourceCount ?? 0} indexedFileCount={model.metrics?.indexedFileCount ?? 0} timelineEventCount={model.metrics?.timelineEventCount ?? 0} artifactCount={model.metrics?.artifactCount ?? 0} />
      <div className="flex min-h-0 flex-1">
        <RecentTasksPanel runningJob={model.runningJob} completedJobs={model.completedJobs} partialJobCount={model.partialJobCount} />
        <div className="flex min-h-0 w-1/2 flex-col bg-forensics-panel">
          <DataSourcesPanel dataSources={model.dataSources} editingDataSourceId={model.editingDataSourceId} editingDataSourceName={model.editingDataSourceName} setEditingDataSourceId={model.setEditingDataSourceId} setEditingDataSourceName={model.setEditingDataSourceName} onRename={model.renameDataSource} onDelete={model.deleteDataSource} />
          <RecentObjectsPanel recentObjects={model.recentObjects} />
        </div>
      </div>
    </div>
  );
}
