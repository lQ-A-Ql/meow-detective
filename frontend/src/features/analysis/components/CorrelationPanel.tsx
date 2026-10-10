import { useTranslation } from 'react-i18next';
import { AlertTriangle, BarChart3, CheckCircle2, Database } from 'lucide-react';
import type {
  ReleaseScorecard,
  V2GovernanceSnapshot,
} from '@/types/models';
import { Metric, MessageBlock } from './V2GovernancePanels';

function coverageTone(status: 'covered' | 'review' | 'missing') {
  switch (status) {
    case 'covered':
      return 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text';
    case 'review':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    case 'missing':
      return 'border-forensics-border-strong bg-forensics-panel text-forensics-text-secondary';
    default:
      return 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary';
  }
}

function coverageLabel(status: 'covered' | 'review' | 'missing') {
  switch (status) {
    case 'covered':
      return 'Covered';
    case 'review':
      return 'Review';
    case 'missing':
      return 'Missing';
    default:
      return status;
  }
}

export function ReleaseScorecardPanel({
  scorecard,
  runtimeSummary,
}: {
  scorecard: ReleaseScorecard;
  runtimeSummary: V2GovernanceSnapshot['runtimeSignals'];
}) {
  const { t } = useTranslation();

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <BarChart3 size={16} />
        {t('v2Governance.scorecard.title')}
      </div>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[320px_1fr]">
        <div className="rounded-none bg-transparent p-4">
          <div className="text-[11px] uppercase tracking-wider text-forensics-muted-light">{t('v2Governance.scorecard.overall')}</div>
          <div className="mt-2 flex items-end gap-3">
            <div className="text-[40px] font-light text-forensics-text">{scorecard.totalScore}</div>
            <div className="mb-1 rounded-none border border-forensics-border px-2 py-0.5 font-mono text-[12px] text-forensics-text-tertiary">
              {scorecard.grade}
            </div>
          </div>
          <div className="mt-4 space-y-2">
            <Metric label={t('v2Governance.verification.title')} value={scorecard.verificationScore.toString()} />
            <Metric label={t('v2Governance.scorecard.dimensionCorrelation')} value={scorecard.correlationScore.toString()} />
            <Metric label={t('v2Governance.scorecard.dimensionPerformance')} value={scorecard.performanceScore.toString()} />
            <Metric label={t('v2Governance.security.title')} value={scorecard.securityScore.toString()} />
          </div>
        </div>

        <div className="space-y-4">
          <div className="rounded-none bg-transparent p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
              <Database size={14} />
              {t('v2Governance.scorecard.runtimeSignals')}
            </div>
            <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
              <Metric label={t('v2Governance.scorecard.evidenceSources')} value={runtimeSummary.dataSourceCount.toString()} />
              <Metric label={t('v2Governance.scorecard.hashed')} value={runtimeSummary.hashedDataSourceCount.toString()} />
              <Metric label={t('v2Governance.scorecard.pendingHash')} value={runtimeSummary.pendingHashDataSourceCount.toString()} />
              <Metric label={t('v2Governance.scorecard.reportCount')} value={runtimeSummary.reportCount.toString()} />
              <Metric label={t('v2Governance.scorecard.correlationLeads')} value={runtimeSummary.correlationLeadCount.toString()} />
              <Metric label={t('v2Governance.scorecard.highConfidenceLeads')} value={runtimeSummary.correlationHighConfidenceLeadCount.toString()} />
              <Metric label={t('v2Governance.scorecard.reviewLeads')} value={runtimeSummary.correlationReviewLeadCount.toString()} />
              <Metric label={t('v2Governance.scorecard.correlationClusters')} value={runtimeSummary.correlationClusterCount.toString()} />
              <Metric label={t('correlation.metrics.ruleFamilies')} value={runtimeSummary.correlationRuleFamilyCount.toString()} />
              <Metric label={t('correlation.metrics.coveredFamilies')} value={runtimeSummary.correlationCoveredFamilyCount.toString()} />
              <Metric label={t('v2Governance.scorecard.highConfidenceFamilies')} value={runtimeSummary.correlationHighConfidenceFamilyCount.toString()} />
              <Metric label={t('v2Governance.scorecard.runningJobs')} value={runtimeSummary.runningJobCount.toString()} />
              <Metric label={t('v2Governance.scorecard.partialJobs')} value={runtimeSummary.partialJobCount.toString()} />
              <Metric label={t('v2Governance.scorecard.failedJobs')} value={runtimeSummary.failedJobCount.toString()} />
              <Metric label={t('v2Governance.scorecard.warningDataSources')} value={runtimeSummary.warningDataSourceCount.toString()} />
            </div>
            <div className="mt-3 bg-transparent px-3 py-2 text-[11px] text-forensics-text-tertiary">
              {t('v2Governance.scorecard.snapshotStatus', {
                status: runtimeSummary.correlationSnapshotAvailable
                  ? t('v2Governance.scorecard.snapshotGenerated')
                  : t('v2Governance.scorecard.snapshotNotGenerated'),
              })}
            </div>
            <div className="mt-4 bg-transparent p-3">
              <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Correlation Family Coverage</div>
              <div className="space-y-2">
                {runtimeSummary.correlationFamilyCoverage.map((item) => (
                  <div key={item.family} className="rounded-none bg-transparent px-3 py-2 text-[11px] text-forensics-text-tertiary">
                    <div className="flex items-center justify-between gap-2">
                      <div className="font-light text-forensics-text">{item.displayName}</div>
                      <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${coverageTone(item.status)}`}>
                        {coverageLabel(item.status)}
                      </span>
                    </div>
                    <div className="mt-2 grid grid-cols-2 gap-2 lg:grid-cols-4">
                      <Metric label="Lead" value={item.leadCount.toString()} />
                      <Metric label={t('correlation.metrics.highConfidence')} value={item.highConfidenceLeadCount.toString()} />
                      <Metric label={t('dashboard.status.review')} value={item.reviewLeadCount.toString()} />
                      <Metric label="Cluster" value={item.clusterCount.toString()} />
                    </div>
                    {item.sampleSignals.length > 0 ? (
                      <div className="mt-2 space-y-1 text-[11px] text-forensics-muted">
                        {item.sampleSignals.map((signal) => (
                          <div key={`${item.family}-${signal}`}>{signal}</div>
                        ))}
                      </div>
                    ) : (
                      <div className="mt-2 text-[11px] text-forensics-muted">{t('v2Governance.scorecard.familyEmpty')}</div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          </div>

          <div className="rounded-none bg-transparent p-4">
            <div className="mb-3 text-[12px] font-light text-forensics-text">{t('v2Governance.scorecard.breakdownTitle')}</div>
            <div className="space-y-3">
              {scorecard.breakdown.map((entry) => (
                <div key={entry.dimension} className="bg-transparent p-3">
                  <div className="flex items-center justify-between gap-3">
                    <div className="text-[12px] font-light text-forensics-text">{entry.dimension}</div>
                    <div className="font-mono text-[11px] text-forensics-text-tertiary">
                      {entry.actualScore} / {entry.maxScore}
                    </div>
                  </div>
                  {entry.deductions.length > 0 ? (
                    <div className="mt-2 space-y-1 text-[11px] text-forensics-muted">
                      {entry.deductions.map((item) => (
                        <div key={`${entry.dimension}-${item}`}>{item}</div>
                      ))}
                    </div>
                  ) : (
                    <div className="mt-2 text-[11px] text-forensics-muted">{t('v2Governance.scorecard.noDeductions')}</div>
                  )}
                </div>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <MessageBlock
              icon={<AlertTriangle size={14} className="text-forensics-warning-text" />}
              title={t('v2Governance.scorecard.blockers')}
              items={scorecard.blockers}
              empty={t('v2Governance.scorecard.noBlockers')}
            />
            <MessageBlock
              icon={<CheckCircle2 size={14} className="text-forensics-success-text" />}
              title={t('v2Governance.scorecard.residualRisks')}
              items={scorecard.residualRisks}
              empty={t('v2Governance.scorecard.noResidualRisks')}
            />
          </div>
        </div>
      </div>
    </section>
  );
}
