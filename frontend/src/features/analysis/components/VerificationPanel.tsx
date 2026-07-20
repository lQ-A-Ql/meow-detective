import { Gauge, Target, Database } from 'lucide-react';
import type {
  BenchmarkRequiredCheck,
  BenchmarkSummary,
  ParserSupportMatrixEntry,
  VerificationChainStatus,
} from '@/types/models';
import { Metric } from './V2GovernancePanels';

function cardTone(result: VerificationChainStatus['result']) {
  if (result === 'failed') return 'border-forensics-error-border bg-forensics-error-bg';
  if (result === 'partial') return 'border-forensics-warning-border bg-forensics-warning-bg';
  if (result === 'pending') return 'border-forensics-border bg-forensics-panel';
  return 'border-forensics-border bg-forensics-surface';
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

export function maturityLabel(value: VerificationChainStatus['maturity']) {
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

export function VerificationDashboard({ snapshot }: { snapshot: import('@/types/models').V2GovernanceSnapshot }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Target size={16} />
        可信验证
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {snapshot.verificationChains.map((chain) => (
          <div key={chain.chain} className={`rounded-none border p-4 ${cardTone(chain.result)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-light text-forensics-text">{chain.displayName}</div>
                <div className="mt-1 font-mono text-[10px] text-forensics-muted">{chain.chain}</div>
              </div>
              <span className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[10px] font-mono text-forensics-text-tertiary">
                {resultLabel(chain.result)}
              </span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
              <Metric label="成熟度" value={maturityLabel(chain.maturity)} />
              <Metric label="保证级别" value={guaranteeLabel(chain.guaranteeLevel)} />
              <Metric label="样本层级" value={chain.fixtureTier} />
              <Metric label="样本数" value={chain.verifiedSampleCount.toString()} />
            </div>
            <div className="mt-3 text-[11px] text-forensics-text-tertiary">
              expected JSON: <span className="font-mono">{chain.expectedJsonVersion}</span>
            </div>
            {chain.notes.length > 0 ? (
              <ul className="mt-2 space-y-1 text-[11px] text-forensics-muted">
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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Gauge size={16} />
        Benchmark 基线
      </div>
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
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
            <div key={`${scenario.datasetLevel}-${scenario.scenario}`} className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
              <div className="text-[12px] font-light text-forensics-text">{scenario.scenario}</div>
              <div className="mt-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{scenario.datasetLevel}</div>
              <div className="mt-3 text-[22px] font-light text-forensics-text">{scenario.p95Ms}ms</div>
              <div className="mt-1 text-[11px] text-forensics-muted">
                峰值内存 {scenario.memoryPeakMb ? `${scenario.memoryPeakMb}MB` : '-'}
              </div>
            </div>
          ))}
        </div>
        <div className="mt-4 rounded-none border border-forensics-border-light bg-forensics-surface p-3">
          <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Required Checks</div>
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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Database size={16} />
        支持矩阵明细
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {entries.map((entry) => (
          <div key={entry.chain} className="rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-light text-forensics-text">{entry.chain}</div>
                <div className="mt-1 text-[11px] text-forensics-muted">{entry.baseline}</div>
              </div>
              <span className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[10px] font-mono text-forensics-text-tertiary">
                {maturityLabel(entry.maturity)}
              </span>
            </div>
            <div className="mt-3 rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
              {entry.guaranteeSummary}
            </div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider text-forensics-muted-light">Verified Samples</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {entry.verifiedSamples.map((sample) => (
                  <span key={`${entry.chain}-${sample}`} className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-1 text-[10px] text-forensics-text-tertiary">
                    {sample}
                  </span>
                ))}
              </div>
            </div>
            {entry.notes.length > 0 ? (
              <div className="mt-3 space-y-1 text-[11px] text-forensics-muted">
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

function benchmarkCheckTone(status: BenchmarkRequiredCheck['status']) {
  switch (status) {
    case 'covered':
      return 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text';
    case 'missing':
      return 'border-forensics-border-strong bg-forensics-panel text-forensics-text-secondary';
    case 'exceeded':
      return 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text';
    default:
      return 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary';
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
    <div className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="font-light text-forensics-text">{check.scenario}</div>
          <div className="mt-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{check.datasetLevel}</div>
        </div>
        <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${benchmarkCheckTone(check.status)}`}>
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
