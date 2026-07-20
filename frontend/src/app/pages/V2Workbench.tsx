import { RefreshCw } from 'lucide-react';
import { AnalysisEmptyState, AnalysisErrorBanner, AnalysisLoadingPanel } from '@/features/analysis/components/AnalysisPanels';
import { CorrelationWorkspace } from '@/features/analysis/components/CorrelationWorkspace';
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
import { useCurrentCase } from '@/features/case/hooks';
import { useCorrelationSnapshot, useV2GovernanceSnapshot } from '@/features/analysis/hooks';
import { Button } from '@/app/components/ui/button';
import { errorMessage as formatErrorMessage } from '@/lib/errors';

function errorMessage(error: unknown) {
  return formatErrorMessage(error);
}

export function V2Workbench() {
  const currentCase = useCurrentCase();
  const snapshot = useV2GovernanceSnapshot();
  const correlation = useCorrelationSnapshot();

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading;
  const error = currentCase.error ?? snapshot.error ?? correlation.error;

  async function refresh() {
    await Promise.all([snapshot.refetch(), correlation.refetch()]);
  }

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="font-serif text-xl tracking-tight text-forensics-text">V2 治理工作台</div>
            <div className="mt-1 font-mono text-[11px] text-forensics-muted">
              可信验证 / 支持矩阵 / Benchmark / 安全治理 / 发布评分卡
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={refresh}
              disabled={!hasCase || loading}
              className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"
            >
              <RefreshCw size={14} className={snapshot.isFetching ? 'opacity-70' : ''} />
              刷新
            </Button>
          </div>
        </div>
      </div>

      {!hasCase && currentCase.isSuccess ? (
        <AnalysisEmptyState />
      ) : loading ? (
        <AnalysisLoadingPanel text="正在加载 V2 治理快照..." />
      ) : (
        <div className="flex-1 space-y-6 overflow-auto p-6">
          {error ? <AnalysisErrorBanner message={errorMessage(error)} onRetry={refresh} /> : null}
          {snapshot.data ? (
            <>
              <GovernanceOverviewStrip snapshot={snapshot.data} />
              <GovernanceFactSourcesPanel snapshot={snapshot.data} />
              <GovernanceRuntimeResultsPanel snapshot={snapshot.data} />
              <VerificationDashboard snapshot={snapshot.data} />
              <SupportMatrixPanel entries={snapshot.data.supportMatrixEntries} />
              <KnownLimitationsPanel items={snapshot.data.knownLimitations} />
              <BenchmarkPanel benchmark={snapshot.data.benchmark} />
              <SecurityAuditPanel security={snapshot.data.security} />
              <ErrorTaxonomyPanel entries={snapshot.data.errorTaxonomyEntries} />
              <ReleaseGatePanel entries={snapshot.data.releaseGates} />
              <ReleaseScorecardPanel
                scorecard={snapshot.data.releaseScorecard}
                runtimeSummary={snapshot.data.runtimeSignals}
              />
              {correlation.data ? (
                <CorrelationWorkspace
                  snapshot={correlation.data}
                  onRefresh={refresh}
                  refreshing={snapshot.isFetching || correlation.isFetching}
                />
              ) : (
                <AnalysisLoadingPanel text="正在加载关联分析快照..." />
              )}
            </>
          ) : (
            <AnalysisLoadingPanel text="当前案件尚未返回治理快照。" />
          )}
        </div>
      )}
    </div>
  );
}
