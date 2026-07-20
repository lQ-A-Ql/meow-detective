import { AlertTriangle, CheckCircle2, Database, Shield } from 'lucide-react';
import { MetricCard } from '@/components/data-display';
import type {
  ErrorTaxonomyEntry,
  ReleaseGateEntry,
  SecurityAuditEntry,
  SecurityAuditSummary,
  V2GovernanceSnapshot,
} from '@/types/models';

// Re-export all split panels so existing consumers continue to work
export { VerificationDashboard, BenchmarkPanel, SupportMatrixPanel } from './VerificationPanel';
export { maturityLabel } from './VerificationPanel';
export { KnownLimitationsPanel } from './LimitationsPanel';
export { ReleaseScorecardPanel } from './CorrelationPanel';

// ── Shared helper components (used by split panels and local panels) ──

export function Metric({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} mono={false} size="sm" className="bg-forensics-surface" />;
}

export function OverviewCard({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} mono={false} align="center" size="md" />;
}

export function MessageBlock({
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
    <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
      <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-text">
        {icon}
        {title}
      </div>
      {items.length > 0 ? (
        <div className="space-y-2 text-[11px] text-forensics-text-tertiary">
          {items.map((item) => (
            <div key={item}>{item}</div>
          ))}
        </div>
      ) : (
        <div className="text-[11px] text-forensics-muted">{empty}</div>
      )}
    </div>
  );
}

// ── Local helper functions ──

function releaseGateTone(status: ReleaseGateEntry['status']) {
  if (status === 'blocked') return 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text';
  if (status === 'warning') return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
  return 'border-forensics-border bg-forensics-surface text-forensics-text';
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

// ── Remaining panel components ──

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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Shield size={16} />
        安全治理
      </div>
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {rows.map(([label, value]) => (
            <Metric key={label} label={label} value={value} />
          ))}
        </div>
        <div className="mt-4 rounded-none border border-forensics-border-light bg-forensics-surface p-3">
          <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Recent Audit Entries</div>
          {security.recentAuditEntries.length > 0 ? (
            <div className="space-y-2">
              {security.recentAuditEntries.map((entry) => (
                <SecurityAuditRow key={`${entry.action}-${entry.createdAt}-${entry.resourceId ?? 'none'}`} entry={entry} />
              ))}
            </div>
          ) : (
            <div className="text-[11px] text-forensics-muted">当前案件尚未写入安全相关审计记录。</div>
          )}
        </div>
        {security.notes.length > 0 ? (
          <div className="mt-4 rounded-none border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text">
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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <AlertTriangle size={16} />
        错误分类与脱敏
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {entries.map((entry) => (
          <div key={entry.category} className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-light text-forensics-text">{entry.category}</div>
                <div className="mt-1 text-[11px] text-forensics-muted">severity: {entry.severity}</div>
              </div>
              <span className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[10px] font-mono text-forensics-text-tertiary">
                {entry.recoverable ? 'recoverable' : 'non-recoverable'}
              </span>
            </div>
            <div className="mt-3 text-[11px] text-forensics-text-tertiary">
              <span className="font-light text-forensics-text">脱敏规则：</span>
              {entry.redactionRule}
            </div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider text-forensics-muted-light">Examples</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {entry.examples.map((sample) => (
                  <span key={`${entry.category}-${sample}`} className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-1 text-[10px] text-forensics-text-tertiary">
                    {sample}
                  </span>
                ))}
              </div>
            </div>
            {entry.notes.length > 0 ? (
              <div className="mt-3 space-y-1 text-[11px] text-forensics-muted">
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

export function ReleaseGatePanel({ entries }: { entries: ReleaseGateEntry[] }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <CheckCircle2 size={16} />
        发布门禁
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {entries.map((entry) => (
          <div key={entry.gateId} className={`rounded-none border p-4 ${releaseGateTone(entry.status)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-light">{entry.title}</div>
                <div className="mt-1 font-mono text-[10px] opacity-70">{entry.gateId}</div>
              </div>
              <span className="rounded-none border border-current/20 bg-forensics-surface/70 px-2 py-0.5 text-[10px] font-mono">
                {releaseGateLabel(entry.status)}
              </span>
            </div>
            <div className="mt-3 text-[11px]">
              <span className="font-light">Evidence：</span>
              {entry.evidence}
            </div>
            <div className="mt-2 text-[11px] opacity-90">{entry.detail}</div>
          </div>
        ))}
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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Database size={16} />
        治理事实源
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {snapshot.factSources.map((source) => (
          <div key={`${source.area}-${source.factFile}`} className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-light text-forensics-text">{source.area}</div>
                <div className="mt-1 font-mono text-[10px] text-forensics-muted">{source.factFile}</div>
              </div>
              <span className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[10px] font-mono text-forensics-text-tertiary">
                {source.factKind}
              </span>
            </div>
            <div className="mt-3 text-[11px] text-forensics-muted">最近校验：{source.lastVerifiedAt}</div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider text-forensics-muted-light">Derived Outputs</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {source.derivedOutputs.map((item) => (
                  <span key={`${source.area}-${item}`} className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-1 text-[10px] text-forensics-text-tertiary">
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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <CheckCircle2 size={16} />
        最近一次治理运行结果
      </div>
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
        <div className="mb-4 text-[11px] text-forensics-muted">最近校验：{snapshot.runtimeResults.checkedAt}</div>
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          {snapshot.runtimeResults.checks.map((check) => (
            <div key={check.checkId} className={`rounded-none border p-4 ${releaseGateTone(check.status)}`}>
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-[13px] font-light">{check.title}</div>
                  <div className="mt-1 font-mono text-[10px] opacity-70">{check.checkId}</div>
                </div>
                <span className="rounded-none border border-current/20 bg-forensics-surface/70 px-2 py-0.5 text-[10px] font-mono">
                  {releaseGateLabel(check.status)}
                </span>
              </div>
              <div className="mt-3 text-[11px]">
                <span className="font-light">Evidence：</span>
                {check.evidence}
              </div>
              <div className="mt-2 text-[11px] opacity-90">{check.detail}</div>
              {check.subChecks.length > 0 ? (
                <div className="mt-3 rounded-none border border-forensics-border-light bg-forensics-surface p-3">
                  <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Sub Checks</div>
                  <div className="space-y-2">
                    {check.subChecks.map((subCheck) => (
                      <div
                        key={`${check.checkId}-${subCheck.checkId}`}
                        className={`rounded-none border px-3 py-2 text-[11px] ${releaseGateTone(subCheck.status)}`}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div>
                            <div className="font-light">{subCheck.title}</div>
                            <div className="mt-1 font-mono text-[10px] opacity-70">{subCheck.checkId}</div>
                          </div>
                          <span className="rounded-none border border-current/20 bg-forensics-surface/70 px-2 py-0.5 text-[10px] font-mono">
                            {releaseGateLabel(subCheck.status)}
                          </span>
                        </div>
                        <div className="mt-2">
                          <span className="font-light">Evidence：</span>
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

// ── Local helper components ──

function SecurityAuditRow({ entry }: { entry: SecurityAuditEntry }) {
  return (
    <div className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-[10px] text-forensics-muted-light">{entry.action}</span>
        <span
          className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${
            entry.sensitive
              ? 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text'
              : 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary'
          }`}
        >
          {entry.sensitive ? 'Sensitive' : 'Standard'}
        </span>
      </div>
      <div className="mt-1 break-all text-forensics-text">
        {entry.resourceType}
        {entry.resourceId ? ` · ${entry.resourceId}` : ''}
      </div>
      {entry.summary ? <div className="mt-1 text-forensics-muted">{entry.summary}</div> : null}
      <div className="mt-1 text-[10px] text-forensics-muted-light">{entry.createdAt}</div>
    </div>
  );
}
