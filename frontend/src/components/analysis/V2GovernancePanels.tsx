import { AlertTriangle, BarChart3, CheckCircle2, Database, Gauge, Shield, Target } from 'lucide-react';
import type {
  BenchmarkRequiredCheck,
  BenchmarkSummary,
  ErrorTaxonomyEntry,
  KnownLimitation,
  ParserSupportMatrixEntry,
  ReleaseGateEntry,
  ReleaseScorecard,
  SecurityAuditEntry,
  SecurityAuditSummary,
  V2GovernanceSnapshot,
  VerificationChainStatus,
} from '@/types/models';

function cardTone(result: VerificationChainStatus['result']) {
  if (result === 'failed') return 'border-red-200 bg-red-50';
  if (result === 'partial') return 'border-amber-200 bg-amber-50';
  if (result === 'pending') return 'border-slate-200 bg-slate-50';
  return 'border-[#e0e0e0] bg-white';
}

function resultLabel(result: VerificationChainStatus['result']) {
  switch (result) {
    case 'passed':
      return '通过';
    case 'partial':
      return '部分通过';
    case 'pending':
      return '待验证';
    case 'failed':
      return '失败';
    default:
      return result;
  }
}

function maturityLabel(value: VerificationChainStatus['maturity']) {
  switch (value) {
    case 'ga':
      return 'GA';
    case 'beta':
      return 'Beta';
    case 'experimental':
      return 'Experimental';
    case 'unsupported':
      return 'Unsupported';
    default:
      return value;
  }
}

function guaranteeLabel(value: VerificationChainStatus['guaranteeLevel']) {
  switch (value) {
    case 'guaranteed':
      return 'Guaranteed';
    case 'bestEffort':
      return 'Best-effort';
    case 'experimental':
      return 'Experimental';
    case 'notGuaranteed':
      return 'Not guaranteed';
    default:
      return value;
  }
}

export function VerificationDashboard({ snapshot }: { snapshot: V2GovernanceSnapshot }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Target size={16} />
        可信验证
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {snapshot.verificationChains.map((chain) => (
          <div key={chain.chain} className={`rounded border p-4 ${cardTone(chain.result)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold text-[#111]">{chain.displayName}</div>
                <div className="mt-1 font-mono text-[10px] text-[#777]">{chain.chain}</div>
              </div>
              <span className="rounded border border-[#ddd] bg-white px-2 py-0.5 text-[10px] font-mono text-[#555]">
                {resultLabel(chain.result)}
              </span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
              <Metric label="成熟度" value={maturityLabel(chain.maturity)} />
              <Metric label="保证级别" value={guaranteeLabel(chain.guaranteeLevel)} />
              <Metric label="样本层级" value={chain.fixtureTier} />
              <Metric label="样本数" value={chain.verifiedSampleCount.toString()} />
            </div>
            <div className="mt-3 text-[11px] text-[#555]">
              expected JSON: <span className="font-mono">{chain.expectedJsonVersion}</span>
            </div>
            {chain.notes.length > 0 ? (
              <ul className="mt-2 space-y-1 text-[11px] text-[#666]">
                {chain.notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            ) : null}
          </div>
        ))}
      </div>
    </section>
  );
}

export function BenchmarkPanel({ benchmark }: { benchmark: BenchmarkSummary }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Gauge size={16} />
        Benchmark 基线
      </div>
      <div className="rounded border border-[#e0e0e0] bg-white p-4">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <Metric label="宿主基线" value={benchmark.hostProfile} />
          <Metric label="版本" value={benchmark.baselineVersion} />
          <Metric label="最近校验" value={benchmark.lastVerifiedAt} />
        </div>
        <div className="mt-4 grid grid-cols-1 gap-3 md:grid-cols-3">
          <Metric label="必需项已覆盖" value={benchmark.coveredRequiredCount.toString()} />
          <Metric label="必需项缺失" value={benchmark.missingRequiredCount.toString()} />
          <Metric label="超阈值项" value={benchmark.exceededRequiredCount.toString()} />
        </div>
        <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-3">
          {benchmark.scenarios.map((scenario) => (
            <div key={`${scenario.datasetLevel}-${scenario.scenario}`} className="rounded border border-[#eee] bg-[#fcfcfc] p-3">
              <div className="text-[12px] font-medium text-[#111]">{scenario.scenario}</div>
              <div className="mt-1 text-[10px] uppercase tracking-wider text-[#888]">{scenario.datasetLevel}</div>
              <div className="mt-3 text-[22px] font-semibold text-[#111]">{scenario.p95Ms}ms</div>
              <div className="mt-1 text-[11px] text-[#666]">
                峰值内存 {scenario.memoryPeakMb ? `${scenario.memoryPeakMb}MB` : '-'}
              </div>
            </div>
          ))}
        </div>
        <div className="mt-4 rounded border border-[#eee] bg-[#fcfcfc] p-3">
          <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">Required Checks</div>
          <div className="space-y-2">
            {benchmark.requiredChecks.map((check) => (
              <BenchmarkRequiredCheckRow
                key={`${check.datasetLevel}-${check.scenario}`}
                check={check}
              />
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

export function SupportMatrixPanel({ entries }: { entries: ParserSupportMatrixEntry[] }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Database size={16} />
        支持矩阵明细
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {entries.map((entry) => (
          <div key={entry.chain} className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold text-[#111]">{entry.chain}</div>
                <div className="mt-1 text-[11px] text-[#666]">{entry.baseline}</div>
              </div>
              <span className="rounded border border-[#ddd] bg-[#fcfcfc] px-2 py-0.5 text-[10px] font-mono text-[#555]">
                {maturityLabel(entry.maturity)}
              </span>
            </div>
            <div className="mt-3 rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2 text-[11px] text-[#555]">
              {entry.guaranteeSummary}
            </div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider text-[#888]">Verified Samples</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {entry.verifiedSamples.map((sample) => (
                  <span key={`${entry.chain}-${sample}`} className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555]">
                    {sample}
                  </span>
                ))}
              </div>
            </div>
            {entry.notes.length > 0 ? (
              <div className="mt-3 space-y-1 text-[11px] text-[#666]">
                {entry.notes.map((note) => (
                  <div key={`${entry.chain}-${note}`}>{note}</div>
                ))}
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </section>
  );
}

function knownLimitationTone(status: KnownLimitation['status']) {
  switch (status) {
    case 'unsupported':
      return 'border-red-200 bg-red-50 text-red-800';
    case 'notGuaranteed':
      return 'border-amber-200 bg-amber-50 text-amber-900';
    case 'partial':
      return 'border-slate-200 bg-slate-50 text-slate-700';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

function knownLimitationLabel(status: KnownLimitation['status']) {
  switch (status) {
    case 'unsupported':
      return 'Unsupported';
    case 'notGuaranteed':
      return 'Not Guaranteed';
    case 'partial':
      return 'Partial';
    default:
      return status;
  }
}

export function KnownLimitationsPanel({ items }: { items: KnownLimitation[] }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <AlertTriangle size={16} />
        已知限制
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {items.map((item) => (
          <div key={`${item.category}-${item.item}`} className={`rounded border p-4 ${knownLimitationTone(item.status)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold">{item.item}</div>
                <div className="mt-1 text-[11px] opacity-80">{item.category}</div>
              </div>
              <span className="rounded border border-current/20 bg-white/70 px-2 py-0.5 text-[10px] font-mono">
                {knownLimitationLabel(item.status)}
              </span>
            </div>
            <div className="mt-3 text-[11px]">{item.summary}</div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider opacity-70">Affected Chains</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {item.affectedChains.map((chain) => (
                  <span key={`${item.item}-${chain}`} className="rounded border border-current/20 bg-white/70 px-2 py-1 text-[10px] font-mono">
                    {chain}
                  </span>
                ))}
              </div>
            </div>
            <div className="mt-3 text-[10px] opacity-70">{item.sourceDoc}</div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function SecurityAuditPanel({ security }: { security: SecurityAuditSummary }) {
  const rows = [
    ['导出默认覆盖', security.exportOverwriteDefault ? '开启' : '关闭'],
    ['导出路径防护', security.exportPathGuardEnabled ? '开启' : '关闭'],
    ['stdio 白名单', security.stdioCommandWhitelistEnforced ? '开启' : '关闭'],
    ['SSE 协议限制', security.sseHttpsOnly ? 'http/https only' : '未收紧'],
    ['嵌入凭据阻断', security.embeddedCredentialsBlocked ? '开启' : '关闭'],
    ['媒体句柄作用域', security.mediaHandleScoped ? 'case scoped' : '未约束'],
    ['错误脱敏', security.errorRedactionEnabled ? '开启' : '关闭'],
    ['审计记录必填', security.auditLogRequired ? '开启' : '关闭'],
    ['审计事件数', security.auditEventCount.toString()],
    ['敏感事件数', security.sensitiveAuditEventCount.toString()],
  ];

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Shield size={16} />
        安全治理
      </div>
      <div className="rounded border border-[#e0e0e0] bg-white p-4">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {rows.map(([label, value]) => (
            <Metric key={label} label={label} value={value} />
          ))}
        </div>
        <div className="mt-4 rounded border border-[#eee] bg-[#fcfcfc] p-3">
          <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">Recent Audit Entries</div>
          {security.recentAuditEntries.length > 0 ? (
            <div className="space-y-2">
              {security.recentAuditEntries.map((entry) => (
                <SecurityAuditRow key={`${entry.action}-${entry.createdAt}-${entry.resourceId ?? 'none'}`} entry={entry} />
              ))}
            </div>
          ) : (
            <div className="text-[11px] text-[#777]">当前案件尚未写入安全相关审计记录。</div>
          )}
        </div>
        {security.notes.length > 0 ? (
          <div className="mt-4 rounded border border-amber-200 bg-amber-50 p-3 text-[11px] text-amber-900">
            {security.notes.map((note) => (
              <div key={note}>{note}</div>
            ))}
          </div>
        ) : null}
      </div>
    </section>
  );
}

export function ErrorTaxonomyPanel({ entries }: { entries: ErrorTaxonomyEntry[] }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <AlertTriangle size={16} />
        错误分类与脱敏
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {entries.map((entry) => (
          <div key={entry.category} className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold text-[#111]">{entry.category}</div>
                <div className="mt-1 text-[11px] text-[#666]">severity: {entry.severity}</div>
              </div>
              <span className="rounded border border-[#ddd] bg-[#fcfcfc] px-2 py-0.5 text-[10px] font-mono text-[#555]">
                {entry.recoverable ? 'recoverable' : 'non-recoverable'}
              </span>
            </div>
            <div className="mt-3 text-[11px] text-[#555]">
              <span className="font-medium text-[#111]">脱敏规则：</span>
              {entry.redactionRule}
            </div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider text-[#888]">Examples</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {entry.examples.map((sample) => (
                  <span key={`${entry.category}-${sample}`} className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555]">
                    {sample}
                  </span>
                ))}
              </div>
            </div>
            {entry.notes.length > 0 ? (
              <div className="mt-3 space-y-1 text-[11px] text-[#666]">
                {entry.notes.map((note) => (
                  <div key={`${entry.category}-${note}`}>{note}</div>
                ))}
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </section>
  );
}

function releaseGateTone(status: ReleaseGateEntry['status']) {
  if (status === 'blocked') return 'border-red-200 bg-red-50 text-red-800';
  if (status === 'warning') return 'border-amber-200 bg-amber-50 text-amber-900';
  return 'border-[#e0e0e0] bg-white text-[#111]';
}

function releaseGateLabel(status: ReleaseGateEntry['status']) {
  switch (status) {
    case 'passed':
      return 'Passed';
    case 'warning':
      return 'Warning';
    case 'blocked':
      return 'Blocked';
    default:
      return status;
  }
}

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

export function ReleaseGatePanel({ entries }: { entries: ReleaseGateEntry[] }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <CheckCircle2 size={16} />
        发布门禁
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {entries.map((entry) => (
          <div key={entry.gateId} className={`rounded border p-4 ${releaseGateTone(entry.status)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold">{entry.title}</div>
                <div className="mt-1 font-mono text-[10px] opacity-70">{entry.gateId}</div>
              </div>
              <span className="rounded border border-current/20 bg-white/70 px-2 py-0.5 text-[10px] font-mono">
                {releaseGateLabel(entry.status)}
              </span>
            </div>
            <div className="mt-3 text-[11px]">
              <span className="font-medium">Evidence：</span>
              {entry.evidence}
            </div>
            <div className="mt-2 text-[11px] opacity-90">{entry.detail}</div>
          </div>
        ))}
      </div>
    </section>
  );
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

export function GovernanceOverviewStrip({ snapshot }: { snapshot: V2GovernanceSnapshot }) {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
      <OverviewCard label="GA 链路" value={snapshot.supportMatrix.gaCount.toString()} />
      <OverviewCard label="Beta 链路" value={snapshot.supportMatrix.betaCount.toString()} />
      <OverviewCard label="文档限制" value={snapshot.supportMatrix.documentedLimitCount.toString()} />
      <OverviewCard label="事实源" value={snapshot.factSources.length.toString()} />
      <OverviewCard label="运行任务" value={snapshot.runtimeSignals.runningJobCount.toString()} />
      <OverviewCard label="部分完成" value={snapshot.runtimeSignals.partialJobCount.toString()} />
      <OverviewCard label="总评分" value={snapshot.releaseScorecard.totalScore.toString()} />
    </div>
  );
}

export function GovernanceFactSourcesPanel({ snapshot }: { snapshot: V2GovernanceSnapshot }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Database size={16} />
        治理事实源
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {snapshot.factSources.map((source) => (
          <div key={`${source.area}-${source.factFile}`} className="rounded border border-[#e0e0e0] bg-white p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold text-[#111]">{source.area}</div>
                <div className="mt-1 font-mono text-[10px] text-[#777]">{source.factFile}</div>
              </div>
              <span className="rounded border border-[#ddd] bg-[#fcfcfc] px-2 py-0.5 text-[10px] font-mono text-[#555]">
                {source.factKind}
              </span>
            </div>
            <div className="mt-3 text-[11px] text-[#666]">最近校验：{source.lastVerifiedAt}</div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider text-[#888]">Derived Outputs</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {source.derivedOutputs.map((item) => (
                  <span key={`${source.area}-${item}`} className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555]">
                    {item}
                  </span>
                ))}
              </div>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function GovernanceRuntimeResultsPanel({ snapshot }: { snapshot: V2GovernanceSnapshot }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <CheckCircle2 size={16} />
        最近一次治理运行结果
      </div>
      <div className="rounded border border-[#e0e0e0] bg-white p-4">
        <div className="mb-4 text-[11px] text-[#666]">最近校验：{snapshot.runtimeResults.checkedAt}</div>
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          {snapshot.runtimeResults.checks.map((check) => (
            <div key={check.checkId} className={`rounded border p-4 ${releaseGateTone(check.status)}`}>
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-[13px] font-semibold">{check.title}</div>
                  <div className="mt-1 font-mono text-[10px] opacity-70">{check.checkId}</div>
                </div>
                <span className="rounded border border-current/20 bg-white/70 px-2 py-0.5 text-[10px] font-mono">
                  {releaseGateLabel(check.status)}
                </span>
              </div>
              <div className="mt-3 text-[11px]">
                <span className="font-medium">Evidence：</span>
                {check.evidence}
              </div>
              <div className="mt-2 text-[11px] opacity-90">{check.detail}</div>
              {check.subChecks.length > 0 ? (
                <div className="mt-3 rounded border border-[#eee] bg-[#fcfcfc] p-3">
                  <div className="mb-2 text-[10px] uppercase tracking-wider text-[#888]">Sub Checks</div>
                  <div className="space-y-2">
                    {check.subChecks.map((subCheck) => (
                      <div
                        key={`${check.checkId}-${subCheck.checkId}`}
                        className={`rounded border px-3 py-2 text-[11px] ${releaseGateTone(subCheck.status)}`}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div>
                            <div className="font-medium">{subCheck.title}</div>
                            <div className="mt-1 font-mono text-[10px] opacity-70">{subCheck.checkId}</div>
                          </div>
                          <span className="rounded border border-current/20 bg-white/70 px-2 py-0.5 text-[10px] font-mono">
                            {releaseGateLabel(subCheck.status)}
                          </span>
                        </div>
                        <div className="mt-2">
                          <span className="font-medium">Evidence：</span>
                          {subCheck.evidence}
                        </div>
                        <div className="mt-1 opacity-90">{subCheck.detail}</div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}
              <div className="mt-2 text-[10px] opacity-70">{check.checkedAt}</div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-[#eee] bg-[#fcfcfc] px-3 py-2">
      <div className="text-[10px] uppercase tracking-wider text-[#888]">{label}</div>
      <div className="mt-1 break-words text-[12px] font-medium text-[#111]">{value}</div>
    </div>
  );
}

function OverviewCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white px-3 py-3 text-center">
      <div className="text-[20px] font-semibold text-[#111]">{value}</div>
      <div className="mt-1 text-[10px] uppercase tracking-wider text-[#888]">{label}</div>
    </div>
  );
}

function MessageBlock({
  icon,
  title,
  items,
  empty,
}: {
  icon: React.ReactNode;
  title: string;
  items: string[];
  empty: string;
}) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white p-4">
      <div className="mb-3 flex items-center gap-2 text-[12px] font-semibold text-[#111]">
        {icon}
        {title}
      </div>
      {items.length > 0 ? (
        <div className="space-y-2 text-[11px] text-[#555]">
          {items.map((item) => (
            <div key={item}>{item}</div>
          ))}
        </div>
      ) : (
        <div className="text-[11px] text-[#777]">{empty}</div>
      )}
    </div>
  );
}

function SecurityAuditRow({ entry }: { entry: SecurityAuditEntry }) {
  return (
    <div className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-[10px] text-[#888]">{entry.action}</span>
        <span
          className={`rounded border px-2 py-0.5 text-[10px] font-mono ${
            entry.sensitive
              ? 'border-amber-200 bg-amber-50 text-amber-900'
              : 'border-[#ddd] bg-[#fcfcfc] text-[#555]'
          }`}
        >
          {entry.sensitive ? 'Sensitive' : 'Standard'}
        </span>
      </div>
      <div className="mt-1 break-all text-[#111]">
        {entry.resourceType}
        {entry.resourceId ? ` · ${entry.resourceId}` : ''}
      </div>
      {entry.summary ? <div className="mt-1 text-[#666]">{entry.summary}</div> : null}
      <div className="mt-1 text-[10px] text-[#888]">{entry.createdAt}</div>
    </div>
  );
}

function benchmarkCheckTone(status: BenchmarkRequiredCheck['status']) {
  switch (status) {
    case 'covered':
      return 'border-[#0d7a32] bg-[#effaf2] text-[#0d7a32]';
    case 'missing':
      return 'border-[#667085] bg-[#f8fafc] text-[#475467]';
    case 'exceeded':
      return 'border-[#b42318] bg-[#fef3f2] text-[#b42318]';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

function benchmarkCheckLabel(status: BenchmarkRequiredCheck['status']) {
  switch (status) {
    case 'covered':
      return 'Covered';
    case 'missing':
      return 'Missing';
    case 'exceeded':
      return 'Exceeded';
    default:
      return status;
  }
}

function BenchmarkRequiredCheckRow({ check }: { check: BenchmarkRequiredCheck }) {
  const measured =
    typeof check.measuredP95Ms === 'number' ? `${check.measuredP95Ms}ms` : '未采集';

  return (
    <div className="rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="font-medium text-[#111]">{check.scenario}</div>
          <div className="mt-1 text-[10px] uppercase tracking-wider text-[#888]">{check.datasetLevel}</div>
        </div>
        <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${benchmarkCheckTone(check.status)}`}>
          {benchmarkCheckLabel(check.status)}
        </span>
      </div>
      <div className="mt-2 grid grid-cols-2 gap-2">
        <Metric label="阈值 p95" value={`${check.thresholdP95Ms}ms`} />
        <Metric label="实测 p95" value={measured} />
      </div>
    </div>
  );
}
