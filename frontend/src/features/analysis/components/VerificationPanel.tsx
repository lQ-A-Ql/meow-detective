import { useTranslation } from 'react-i18next';
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

function resultLabel(
  result: VerificationChainStatus['result'],
  t: ReturnType<typeof useTranslation>['t'],
) {
  switch (result) {
    case 'passed':
      return t('v2Governance.verification.resultPassed');
    case 'partial':
      return t('v2Governance.verification.resultPartial');
    case 'pending':
      return t('v2Governance.verification.resultPending');
    case 'failed':
      return t('infrastructure.states.failed');
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
  const { t } = useTranslation();

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Target size={16} />
        {t('v2Governance.verification.title')}
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
                {resultLabel(chain.result, t)}
              </span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
              <Metric label={t('v2Governance.verification.maturity')} value={maturityLabel(chain.maturity)} />
              <Metric label={t('v2Governance.verification.guaranteeLevel')} value={guaranteeLabel(chain.guaranteeLevel)} />
              <Metric label={t('v2Governance.verification.fixtureTier')} value={chain.fixtureTier} />
              <Metric label={t('v2Governance.verification.sampleCount')} value={chain.verifiedSampleCount.toString()} />
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
  const { t } = useTranslation();

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Gauge size={16} />
        {t('v2Governance.benchmark.title')}
      </div>
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <Metric label={t('v2Governance.benchmark.hostProfile')} value={benchmark.hostProfile} />
          <Metric label={t('browser.columns.version')} value={benchmark.baselineVersion} />
          <Metric label={t('v2Governance.benchmark.lastVerified')} value={benchmark.lastVerifiedAt} />
        </div>
        <div className="mt-4 grid grid-cols-1 gap-3 md:grid-cols-3">
          <Metric label={t('v2Governance.benchmark.requiredCovered')} value={benchmark.coveredRequiredCount.toString()} />
          <Metric label={t('v2Governance.benchmark.requiredMissing')} value={benchmark.missingRequiredCount.toString()} />
          <Metric label={t('v2Governance.benchmark.exceeded')} value={benchmark.exceededRequiredCount.toString()} />
        </div>
        <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-3">
          {benchmark.scenarios.map((scenario) => (
            <div key={`${scenario.datasetLevel}-${scenario.scenario}`} className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
              <div className="text-[12px] font-light text-forensics-text">{scenario.scenario}</div>
              <div className="mt-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{scenario.datasetLevel}</div>
              <div className="mt-3 text-[22px] font-light text-forensics-text">{scenario.p95Ms}ms</div>
              <div className="mt-1 text-[11px] text-forensics-muted">
                {t('v2Governance.benchmark.memoryPeak', {
                  value: scenario.memoryPeakMb ? `${scenario.memoryPeakMb}MB` : '-',
                })}
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
  const { t } = useTranslation();

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <Database size={16} />
        {t('v2Governance.supportMatrix.title')}
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
  const { t } = useTranslation();
  const measured =
    typeof check.measuredP95Ms === 'number'
      ? `${check.measuredP95Ms}ms`
      : t('v2Governance.benchmark.notMeasured');

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
        <Metric label={t('v2Governance.benchmark.thresholdP95')} value={`${check.thresholdP95Ms}ms`} />
        <Metric label={t('v2Governance.benchmark.measuredP95')} value={measured} />
      </div>
    </div>
  );
}
