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
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <BarChart3 size={16} />
        发布评分卡
      </div>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[320px_1fr]">
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
          <div className="text-[11px] uppercase tracking-wider text-forensics-muted-light">总评</div>
          <div className="mt-2 flex items-end gap-3">
            <div className="text-[40px] font-light text-forensics-text">{scorecard.totalScore}</div>
            <div className="mb-1 rounded-none border border-forensics-border px-2 py-0.5 font-mono text-[12px] text-forensics-text-tertiary">
              {scorecard.grade}
            </div>
          </div>
          <div className="mt-4 space-y-2">
            <Metric label="可信验证" value={scorecard.verificationScore.toString()} />
            <Metric label="关联分析" value={scorecard.correlationScore.toString()} />
            <Metric label="性能稳定性" value={scorecard.performanceScore.toString()} />
            <Metric label="安全治理" value={scorecard.securityScore.toString()} />
          </div>
        </div>

        <div className="space-y-4">
          <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
              <Database size={14} />
              运行信号
            </div>
            <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
              <Metric label="证据源" value={runtimeSummary.dataSourceCount.toString()} />
              <Metric label="已哈希" value={runtimeSummary.hashedDataSourceCount.toString()} />
              <Metric label="待哈希" value={runtimeSummary.pendingHashDataSourceCount.toString()} />
              <Metric label="报告数" value={runtimeSummary.reportCount.toString()} />
              <Metric label="关联 Lead" value={runtimeSummary.correlationLeadCount.toString()} />
              <Metric label="高置信 Lead" value={runtimeSummary.correlationHighConfidenceLeadCount.toString()} />
              <Metric label="待复核 Lead" value={runtimeSummary.correlationReviewLeadCount.toString()} />
              <Metric label="关联 Cluster" value={runtimeSummary.correlationClusterCount.toString()} />
              <Metric label="规则家族" value={runtimeSummary.correlationRuleFamilyCount.toString()} />
              <Metric label="已覆盖家族" value={runtimeSummary.correlationCoveredFamilyCount.toString()} />
              <Metric label="高置信家族" value={runtimeSummary.correlationHighConfidenceFamilyCount.toString()} />
              <Metric label="运行中任务" value={runtimeSummary.runningJobCount.toString()} />
              <Metric label="部分完成任务" value={runtimeSummary.partialJobCount.toString()} />
              <Metric label="失败任务" value={runtimeSummary.failedJobCount.toString()} />
              <Metric label="warning 证据源" value={runtimeSummary.warningDataSourceCount.toString()} />
            </div>
            <div className="mt-3 rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
              关联快照状态：{runtimeSummary.correlationSnapshotAvailable ? '已生成' : '未生成'}
            </div>
            <div className="mt-4 rounded-none border border-forensics-border-light bg-forensics-surface p-3">
              <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Correlation Family Coverage</div>
              <div className="space-y-2">
                {runtimeSummary.correlationFamilyCoverage.map((item) => (
                  <div key={item.family} className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
                    <div className="flex items-center justify-between gap-2">
                      <div className="font-light text-forensics-text">{item.displayName}</div>
                      <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${coverageTone(item.status)}`}>
                        {coverageLabel(item.status)}
                      </span>
                    </div>
                    <div className="mt-2 grid grid-cols-2 gap-2 lg:grid-cols-4">
                      <Metric label="Lead" value={item.leadCount.toString()} />
                      <Metric label="高置信" value={item.highConfidenceLeadCount.toString()} />
                      <Metric label="待复核" value={item.reviewLeadCount.toString()} />
                      <Metric label="Cluster" value={item.clusterCount.toString()} />
                    </div>
                    {item.sampleSignals.length > 0 ? (
                      <div className="mt-2 space-y-1 text-[11px] text-forensics-muted">
                        {item.sampleSignals.map((signal) => (
                          <div key={`${item.family}-${signal}`}>{signal}</div>
                        ))}
                      </div>
                    ) : (
                      <div className="mt-2 text-[11px] text-forensics-muted">当前没有可展示的该家族规则命中。</div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          </div>

          <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-3 text-[12px] font-light text-forensics-text">评分拆解</div>
            <div className="space-y-3">
              {scorecard.breakdown.map((entry) => (
                <div key={entry.dimension} className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
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
                    <div className="mt-2 text-[11px] text-forensics-muted">当前无扣分项</div>
                  )}
                </div>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <MessageBlock
              icon={<AlertTriangle size={14} className="text-forensics-warning-text" />}
              title="阻断项"
              items={scorecard.blockers}
              empty="当前无阻断项"
            />
            <MessageBlock
              icon={<CheckCircle2 size={14} className="text-forensics-success-text" />}
              title="残余风险"
              items={scorecard.residualRisks}
              empty="当前无残余风险"
            />
          </div>
        </div>
      </div>
    </section>
  );
}
