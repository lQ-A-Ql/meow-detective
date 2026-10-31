import { useTranslation } from 'react-i18next';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '@/app/components/ui/select';
import { InspectorSection } from '@/components/layout/InspectorPane';
import { EvidenceTaskStatus } from '@/features/jobs/components/EvidenceTaskStatus';
import type { useEvidenceDigestModel } from '../hooks/use-evidence-digest-model';
import type { DigestScope } from '@/types/digest';
import { DigestAlgorithmActions } from './DigestAlgorithmActions';

export function EvidenceDigestInspector({ model }: { model: ReturnType<typeof useEvidenceDigestModel> }) {
  const { t } = useTranslation();
  return <InspectorSection title={t('fileBrowser.inspector.sections.hash')}>
    <div className="space-y-2">
      <Select value={model.scope} onValueChange={(scope) => model.setScope(scope as DigestScope)} disabled={model.loading}>
        <SelectTrigger aria-label={t('evidenceDigest.scope')}><SelectValue /></SelectTrigger>
        <SelectContent>{model.scopes.map((scope) => <SelectItem key={scope} value={scope}>{t(`evidenceDigest.scopes.${scope}`)}</SelectItem>)}</SelectContent>
      </Select>
      <DigestAlgorithmActions
        available={model.available}
        loading={model.loading}
        forceRecompute={model.forceRecompute}
        onForceRecomputeChange={model.setForceRecompute}
        onCalculate={model.calculate}
      />
      <EvidenceTaskStatus model={model} />
      {model.historyError ? <div role="alert">{t('evidenceDigest.historyFailed')}</div> : null}
      {model.results.map(({ digest, sourceRevision, stale }) => <div key={digest.algorithm} className="space-y-1 break-all font-mono text-[10px]">
        <div>{t('fileBrowser.inspector.hash.result', { algorithm: digest.algorithm.toUpperCase(), bytes: digest.byteLength })}</div>
        <div>{digest.value}</div><div>{t('evidenceDigest.revision')}: {sourceRevision}</div>
        {stale ? <div className="text-forensics-warning-text">{t('evidenceDigest.stale')}</div> : null}
      </div>)}
    </div>
  </InspectorSection>;
}
