import { ContextBar } from '@/components/layout/ContextBar';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
import { RefreshCw } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { AnalysisEmptyState, AnalysisErrorBanner, AnalysisLoadingPanel } from '@/features/analysis/components/AnalysisPanels';
import { CorrelationWorkspaceContainer } from '@/features/analysis/containers/CorrelationWorkspaceContainer';
import {
  BenchmarkPanel,
  ErrorTaxonomyPanel,
  GovernanceFactSourcesPanel,
  GovernanceOverviewStrip,
  GovernanceRuntimeResultsPanel,
  KnownLimitationsPanel,
  ReleaseGatePanel,
  ReleaseScorecardPanel,
  SecurityAuditPanel,
  SupportMatrixPanel,
  VerificationDashboard,
} from '@/features/analysis/components/V2GovernancePanels';
import type { V2WorkbenchModel } from '@/features/analysis/use-v2-workbench-model';
import { errorMessage } from '@/lib/errors';

interface V2WorkbenchWorkspaceProps {
  model: V2WorkbenchModel;
}

/** Pure V2 governance presentation surface. Data loading belongs to the workspace model. */
export function V2WorkbenchWorkspace({ model }: V2WorkbenchWorkspaceProps) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <ContextBar align="end">
        <Tooltip content={t('v2Governance.workbench.refresh')}><Button aria-label={t('v2Governance.workbench.refresh')} type="button" variant="outline" onClick={model.refresh} disabled={!model.hasCase || model.loading} className="rounded-none border-forensics-border bg-forensics-surface text-[12px] hover:bg-forensics-panel-strong"><RefreshCw size={15} /></Button></Tooltip>
      </ContextBar>
      {!model.hasCase && model.currentCaseIsSuccess ? <AnalysisEmptyState /> : model.loading ? <AnalysisLoadingPanel text={t('v2Governance.workbench.loadingSnapshot')} /> : (
        <ScrollArea className="min-h-0 flex-1" viewportClassName="space-y-6 p-6">
          {model.error ? <AnalysisErrorBanner message={errorMessage(model.error)} onRetry={model.refresh} /> : null}
          {model.snapshot.data ? <>
            <GovernanceOverviewStrip snapshot={model.snapshot.data} />
            <GovernanceFactSourcesPanel snapshot={model.snapshot.data} />
            <GovernanceRuntimeResultsPanel snapshot={model.snapshot.data} />
            <VerificationDashboard snapshot={model.snapshot.data} />
            <SupportMatrixPanel entries={model.snapshot.data.supportMatrixEntries} />
            <KnownLimitationsPanel items={model.snapshot.data.knownLimitations} />
            <BenchmarkPanel benchmark={model.snapshot.data.benchmark} />
            <SecurityAuditPanel security={model.snapshot.data.security} />
            <ErrorTaxonomyPanel entries={model.snapshot.data.errorTaxonomyEntries} />
            <ReleaseGatePanel entries={model.snapshot.data.releaseGates} />
            <ReleaseScorecardPanel scorecard={model.snapshot.data.releaseScorecard} runtimeSummary={model.snapshot.data.runtimeSignals} />
            {model.correlation.data ? <CorrelationWorkspaceContainer snapshot={model.correlation.data} onRefresh={model.refresh} refreshing={model.snapshot.isFetching || model.correlation.isFetching} /> : <AnalysisLoadingPanel text={t('v2Governance.workbench.loadingCorrelation')} />}
          </> : <AnalysisLoadingPanel text={t('v2Governance.workbench.snapshotUnavailable')} />}
        </ScrollArea>
      )}
    </div>
  );
}
