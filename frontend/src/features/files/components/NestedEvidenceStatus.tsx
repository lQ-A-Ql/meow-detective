import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { InspectorSection } from '@/components/layout/InspectorPane';
import { EvidenceTaskStatus } from '@/features/jobs/components/EvidenceTaskStatus';
import type { useNestedEvidenceModel } from '../hooks/use-nested-evidence-model';
export function NestedEvidenceStatus({ model, onOpenResult }: { model: ReturnType<typeof useNestedEvidenceModel>; onOpenResult: () => void }) {
  const { t } = useTranslation();
  if (!model.task && !model.loading && !model.errorText) return null;
  return <InspectorSection title={t('nestedEvidence.title')}><EvidenceTaskStatus model={model} />
    {model.result ? <div className="space-y-2 break-all text-[11px]"><div>{model.result.lineage.nestedFilePath}</div><div>{model.result.derivedDataSourceId}</div><Button shape="row" variant="forensicsOutline" onClick={onOpenResult}>{t('nestedEvidence.openResult')}</Button></div> : null}
  </InspectorSection>;
}
