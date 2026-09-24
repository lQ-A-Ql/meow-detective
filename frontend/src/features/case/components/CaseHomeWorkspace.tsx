import { Upload } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { ConfirmationDialog } from '@/app/components/ui/confirmation-dialog';
import { CaseWelcomeForms } from '@/features/case/components/CaseActions';
import { CaseMetricsStrip, DataSourcesPanel, RecentObjectsPanel, RecentTasksPanel } from '@/features/case/components/CaseOverview';
import type { CaseHomeModel } from '@/features/case/use-case-home-model';
import { ImportDataSourceDialog } from '@/features/import/components/ImportDataSourceDialog';

interface CaseHomeWorkspaceProps { model: CaseHomeModel; }

export function CaseHomeWorkspace({ model }: CaseHomeWorkspaceProps) {
  const { t } = useTranslation();
  const deleteTarget = model.deleteTarget;
  const confirmation = (
    <ConfirmationDialog
      open={Boolean(deleteTarget)}
      onOpenChange={(open) => { if (!open) model.setDeleteTarget(undefined); }}
      title={deleteTarget?.kind === 'case' ? t('caseHome.confirm.deleteCaseTitle') : t('caseHome.confirm.deleteDataSourceTitle')}
      description={deleteTarget?.kind === 'case' ? t('caseHome.confirm.deleteCaseDescription', { name: deleteTarget.value.name }) : t('caseHome.confirm.deleteDataSourceDescription', { name: deleteTarget?.value.name ?? '' })}
      cancelLabel={t('caseHome.actions.cancel')}
      confirmLabel={t('caseHome.actions.delete')}
      onConfirm={model.confirmDelete}
      destructive
    />
  );

  if (!model.currentCase) {
    return <>{confirmation}<CaseWelcomeForms caseRoot={model.caseRoot} setCaseRoot={model.updateCaseRoot} caseName={model.caseName} setCaseName={model.setCaseName} onCreateCase={model.createCase} createPending={model.createCasePending} createError={model.createCaseError} openCasePath={model.openCasePath} setOpenCasePath={model.setOpenCasePath} onOpenCase={model.openCase} openPending={model.openCasePending} openError={model.openCaseError} recentCases={model.recentCases} onRequestDeleteCase={model.requestDeleteCase} /></>;
  }

  return <>{confirmation}<div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6"><div className="flex items-start justify-between gap-6"><div><div className="mb-1 font-serif text-2xl tracking-tight text-forensics-text">{t('caseHome.workspace.caseNumber', { number: model.currentCase.number ?? '-' })}</div><div className="font-mono text-[11px] text-forensics-muted">{model.currentCase.name}</div><div className="mt-3 flex flex-wrap gap-2 text-[10px] uppercase tracking-wider text-forensics-muted"><span className="border border-forensics-border-strong bg-forensics-surface px-2 py-1">{t('caseHome.workspace.active')}</span><span className="border border-forensics-border-strong bg-forensics-surface px-2 py-1">{t('caseHome.workspace.dataSources', { count: model.metrics?.dataSourceCount ?? 0 })}</span><span className="border border-forensics-warning-border bg-forensics-surface px-2 py-1">{t('caseHome.workspace.warnings', { count: model.warnings?.length ?? 0 })}</span><span className="border border-forensics-warning-border bg-forensics-surface px-2 py-1">{t('caseHome.workspace.partialJobs', { count: model.partialJobCount })}</span></div></div><div className="flex gap-8 text-right"><div><div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{t('caseHome.workspace.status')}</div><div className="flex items-center justify-end gap-1.5 text-[13px] text-forensics-text"><div className="h-1.5 w-1.5 rounded-none bg-forensics-text" /> {t('caseHome.workspace.activeValue')}</div></div><div><div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{t('caseHome.workspace.createdAt')}</div><div className="font-mono text-[13px] text-forensics-text">{model.currentCase.createdAt}</div></div><div><div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{t('caseHome.workspace.examiner')}</div><div className="text-[13px] text-forensics-text">{model.currentCase.examiner ?? '-'}</div></div><Button type="button" variant="forensicsOutline" size="xs" onClick={() => model.setImportDialogOpen(true)}><Upload size={12} /> {model.importButtonLabel}</Button></div></div></div>
    <ImportDataSourceDialog open={model.importDialogOpen} onOpenChange={model.setImportDialogOpen} onImport={model.importDataSource} importPending={model.importPending} pickSourcePath={model.pickImportSourcePath} pickDirectoryPath={model.pickImportDirectoryPath} listLocalDisks={model.listLocalDisks} />
    <CaseMetricsStrip dataSourceCount={model.metrics?.dataSourceCount ?? 0} indexedFileCount={model.metrics?.indexedFileCount ?? 0} timelineEventCount={model.metrics?.timelineEventCount ?? 0} artifactCount={model.metrics?.artifactCount ?? 0} />
    <div className="flex min-h-0 flex-1"><RecentTasksPanel runningJob={model.runningJob} completedJobs={model.completedJobs} partialJobCount={model.partialJobCount} /><div className="flex min-h-0 w-1/2 flex-col bg-forensics-panel"><DataSourcesPanel dataSources={model.dataSources} hashJobs={model.evidenceHashJobs} editingDataSourceId={model.editingDataSourceId} editingDataSourceName={model.editingDataSourceName} setEditingDataSourceId={model.setEditingDataSourceId} setEditingDataSourceName={model.setEditingDataSourceName} onRename={model.renameDataSource} onRequestDelete={model.requestDeleteDataSource} /><RecentObjectsPanel recentObjects={model.recentObjects} /></div></div>
  </div></>;
}
