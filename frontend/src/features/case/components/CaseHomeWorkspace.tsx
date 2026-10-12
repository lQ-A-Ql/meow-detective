import { WorkspaceFrame } from '@/components/layout/WorkspaceFrame';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { Progress } from '@/app/components/ui/progress';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { Button } from '@/app/components/ui/button';
import { ConfirmationDialog } from '@/app/components/ui/confirmation-dialog';
import { useTranslation } from 'react-i18next';
import { CaseWelcomeContainer } from '@/features/case/containers/CaseWelcomeContainer';
import { DataSourcesPanel } from '@/features/case/components/CaseOverview';
import type { CaseHomeModel } from '@/features/case/use-case-home-model';
import { ImportDataSourceDialog } from '@/features/import/components/ImportDataSourceDialog';
import { BrandWatermark } from '@/components/brand';

interface CaseHomeWorkspaceProps {
  model: CaseHomeModel;
}

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
    return <>{confirmation}<CaseWelcomeContainer caseRoot={model.caseRoot} setCaseRoot={model.updateCaseRoot} caseName={model.caseName} setCaseName={model.setCaseName} onCreateCase={model.createCase} createPending={model.createCasePending} createError={model.createCaseError} openCasePath={model.openCasePath} setOpenCasePath={model.setOpenCasePath} onOpenCase={model.openCase} openPending={model.openCasePending} openError={model.openCaseError} recentCases={model.recentCases} onRequestDeleteCase={model.requestDeleteCase} /></>;
  }

  return (
    <>
      {confirmation}
      <WorkspaceFrame><WorkspaceCard className="flex min-h-0 flex-1 flex-col">
        <section className="relative isolate shrink-0 px-5 pb-6 pt-10 text-center lg:pt-12" aria-labelledby="case-home-title">
          <BrandWatermark motif="profile-portrait" className="watermark-home" />
          <div className="mx-auto max-w-3xl">
            <div className="text-[10px] font-semibold uppercase tracking-[0.2em] text-forensics-red-ink">{t('caseHome.workspace.active')}</div>
            <h1 id="case-home-title" className="mt-2 truncate text-2xl font-semibold tracking-tight text-forensics-ink lg:text-3xl">{model.currentCase.name}</h1>
            <div className="mt-2 flex flex-wrap items-center justify-center gap-2 font-mono text-[10px] text-forensics-ink-3">
              <span>{t('caseHome.workspace.caseNumber', { number: model.currentCase.number ?? '-' })}</span>
              <span className="text-forensics-line-2">·</span>
              <span>{t('caseHome.workspace.examiner')}: {model.currentCase.examiner ?? '-'}</span>
            </div>
            <div className="mt-4 flex justify-center">
              <Tooltip content={model.importButtonLabel}>
                <Button type="button" variant="forensicsPrimary" size="icon" aria-label={model.importButtonLabel} onClick={() => model.setImportDialogOpen(true)}>
                  <ActionIcon name="upload" />
                </Button>
              </Tooltip>
            </div>
          </div>
        </section>

        <ImportDataSourceDialog open={model.importDialogOpen} onOpenChange={model.setImportDialogOpen} onImport={model.importDataSource} importPending={model.importPending} pickSourcePath={model.pickImportSourcePath} pickDirectoryPath={model.pickImportDirectoryPath} listLocalDisks={model.listLocalDisks} />

        <section className="min-h-0 flex-1 overflow-hidden px-4 py-4 lg:px-8" aria-label={t('caseHome.dataSources.title')}>
          <div className="flex h-full min-h-0 flex-col overflow-hidden bg-forensics-paper">
            <div className="flex min-h-0 flex-1 flex-col">
              <DataSourcesPanel dataSources={model.dataSources} showHeader={false} hashJobs={model.evidenceHashJobs} editingDataSourceId={model.editingDataSourceId} editingDataSourceName={model.editingDataSourceName} setEditingDataSourceId={model.setEditingDataSourceId} setEditingDataSourceName={model.setEditingDataSourceName} onRename={model.renameDataSource} onRequestDelete={model.requestDeleteDataSource} logicalDiskDigest={model.logicalDiskDigest} logicalDiskDigestSourceId={model.logicalDiskDigestSourceId} logicalDiskDigestLoading={model.logicalDiskDigestLoading} logicalDiskDigestError={model.logicalDiskDigestErrorText} onCalculateLogicalDiskDigest={model.calculateLogicalDiskDigest} />
            </div>
          </div>
        </section>

        <footer className="shrink-0 bg-forensics-panel-strong px-5 py-3 lg:px-8" aria-label={t('caseHome.import.progressLabel')}>
          <div className="flex items-center justify-between gap-4 text-[11px]">
            <span className="truncate text-forensics-ink-2">{model.runningJob ? model.runningJob.name : t('caseHome.import.idle')}</span>
            <span className="shrink-0 font-mono font-semibold text-forensics-red-ink">{model.runningJob ? `${model.runningJob.progress}%` : '—'}</span>
          </div>
          <Progress value={model.runningJob?.progress ?? 0} className="mt-2 h-2" aria-label={t('caseHome.import.progressLabel')} />
          <div className="mt-1 truncate text-[10px] text-forensics-ink-3">{model.runningJob?.detail ?? t('caseHome.import.idleHint')}</div>
        </footer>
      </WorkspaceCard></WorkspaceFrame>
    </>
  );
}
