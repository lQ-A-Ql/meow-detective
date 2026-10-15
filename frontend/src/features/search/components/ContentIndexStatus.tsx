import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { EvidenceTaskStatus } from '@/features/jobs/components/EvidenceTaskStatus';
import type { useContentIndexModel } from '../use-content-index-model';
const STATUSES = ['indexed', 'binary', 'oversized', 'unreadable', 'unsupportedEncoding', 'notApplicable'] as const;
export function ContentIndexStatus({ model }: { model: ReturnType<typeof useContentIndexModel> }) {
  const { t } = useTranslation();
  return <div className="space-y-2 border-b border-forensics-divider px-4 py-2 text-[11px]">
    <div className="flex items-center gap-3"><Button shape="row" variant="forensicsOutline" disabled={!model.available || model.loading} onClick={model.build}>{t('contentIndex.build')}</Button>
      {!model.available ? <span>{t('contentIndex.selectSource')}</span> : null}
      {model.coverage ? <span>{t(model.coverage.stale ? 'contentIndex.stale' : model.coverage.summary.complete ? 'contentIndex.complete' : 'contentIndex.notBuilt', { expected: model.coverage.summary.expected })}</span> : null}
    </div>
    {model.coverage ? <div className="flex flex-wrap gap-3">{STATUSES.map((status) => <span key={status}>{t(`contentIndex.statuses.${status}`)}: {model.coverage!.summary[status]}</span>)}</div> : null}
    {model.coverageError ? <div role="alert">{t('contentIndex.coverageFailed')}</div> : null}
    <EvidenceTaskStatus model={model} />
  </div>;
}
