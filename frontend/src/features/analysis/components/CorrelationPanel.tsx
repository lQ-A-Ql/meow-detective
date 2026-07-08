import { AlertTriangle, BarChart3, CheckCircle2, Database } from 'lucide-react';
import type {
  ReleaseScorecard,
  V2GovernanceSnapshot,
} from '@/types/models';
import { Metric, MessageBlock } from './V2GovernancePanels';

function coverageTone(status: 'covered' | 'review' | 'missing') {
  switch (status) {
    case 'covered':
      return 'border-[#0d7a32] bg-[#effaf2] text-[#0d7a32]';
    case 'review':
      return 'border-[#b54708] bg-[#fff7ed] text-[#b54708]';
    case 'missing':
      return 'border-[#667085] bg-[#f8fafc] text-[#475467]';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
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
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <BarChart3 size={16} />
        发布评分卡
      </div>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[320px_1fr]">
        <div className="rounded border border-[#e0e0e0] bg-white p-4">
          <div className="text-[11px] uppercase tracking-wider text-[#888]">总评</div>
          <div className="mt-2 flex items-end gap-3">
            <div className="text-[40px] font-semibold text-[#111]">{scorecard.totalScore}</div>
            <div className="mb-1 rounded border border-[#ddd] px-2 py-0.5 font-mono text-[12px] text-[#555]">
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
          <div className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-semibold text-[#111]">
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
            <div className="mt-3 rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2 text-[11px] text-[#555]">
              关联快照状态：{runtimeSummary.correlationSnapshotAvailable ? '已生成' : '未生成'}
            </div>
            <div className="mt-4 rounded border border-[#eee] bg-[#fcfcfc] p-3">
              <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">Correlation Family Coverage</div>
              <div className="space-y-2">
                {runtimeSummary.correlationFamilyCoverage.map((item) => (
                  <div key={item.family} className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
                    <div className="flex items-center justify-between gap-2">
                      <div className="font-medium text-[#111]">{item.displayName}</div>
                      <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${coverageTone(item.status)}`}>
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
                      <div className="mt-2 space-y-1 text-[11px] text-[#666]">
                        {item.sampleSignals.map((signal) => (
                          <div key={`${item.family}-${signal}`}>{signal}</div>
                        ))}
                      </div>
                    ) : (
                      <div className="mt-2 text-[11px] text-[#777]">当前没有可展示的该家族规则命中。</div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          </div>

          <div className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="mb-3 text-[12px] font-semibold text-[#111]">评分拆解</div>
            <div className="space-y-3">
              {scorecard.breakdown.map((entry) => (
                <div key={entry.dimension} className="rounded border border-[#eee] bg-[#fcfcfc] p-3">
                  <div className="flex items-center justify-between gap-3">
                    <div className="text-[12px] font-medium text-[#111]">{entry.dimension}</div>
                    <div className="font-mono text-[11px] text-[#555]">
                      {entry.actualScore} / {entry.maxScore}
                    </div>
                  </div>
                  {entry.deductions.length > 0 ? (
                    <div className="mt-2 space-y-1 text-[11px] text-[#666]">
                      {entry.deductions.map((item) => (
                        <div key={`${entry.dimension}-${item}`}>{item}</div>
                      ))}
                    </div>
                  ) : (
                    <div className="mt-2 text-[11px] text-[#777]">当前无扣分项</div>
                  )}
                </div>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <MessageBlock
              icon={<AlertTriangle size={14} className="text-amber-700" />}
              title="阻断项"
              items={scorecard.blockers}
              empty="当前无阻断项"
            />
            <MessageBlock
              icon={<CheckCircle2 size={14} className="text-[#0d7a32]" />}
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
