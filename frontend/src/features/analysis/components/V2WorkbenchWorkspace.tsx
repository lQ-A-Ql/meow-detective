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
  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div><div className="font-serif text-xl tracking-tight text-forensics-text">V2 治理工作台</div><div className="mt-1 font-mono text-[11px] text-forensics-muted">可信验证 / 支持矩阵 / Benchmark / 安全治理 / 发布评分卡</div></div>
          <Button type="button" variant="outline" onClick={model.refresh} disabled={!model.hasCase || model.loading} className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"><RefreshCw size={14} className={model.snapshot.isFetching ? 'opacity-70' : ''} />刷新</Button>
        </div>
      </div>
      {!model.hasCase && model.currentCaseIsSuccess ? <AnalysisEmptyState /> : model.loading ? <AnalysisLoadingPanel text="正在加载 V2 治理快照..." /> : (
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
            {model.correlation.data ? <CorrelationWorkspaceContainer snapshot={model.correlation.data} onRefresh={model.refresh} refreshing={model.snapshot.isFetching || model.correlation.isFetching} /> : <AnalysisLoadingPanel text="正在加载关联分析快照..." />}
          </> : <AnalysisLoadingPanel text="当前案件尚未返回治理快照。" />}
        </ScrollArea>
      )}
    </div>
  );
}
