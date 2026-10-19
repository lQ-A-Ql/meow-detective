import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { EvidenceTaskStatus } from '@/features/jobs/components/EvidenceTaskStatus';
import type { useContentIndexModel } from '../use-content-index-model';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import type { DataSourceSummary } from '@/types/models';
const STATUSES = ['indexed', 'binary', 'oversized', 'unreadable', 'unsupportedEncoding', 'notApplicable'] as const;
export function ContentIndexStatus({ model, sources, sourceId, onSourceChange }: {
  model: ReturnType<typeof useContentIndexModel>;
  sources: DataSourceSummary[];
  sourceId?: string;
  onSourceChange: (id: string) => void;
}) {
  const { t } = useTranslation();
  return <div className="space-y-2 border-b border-forensics-divider px-4 py-2 text-[11px]">
    <div className="flex flex-wrap items-center gap-3">
      <span className="font-semibold text-forensics-ink-2">{t('contentIndex.title')}</span>
      <Select value={sourceId ?? ''} onValueChange={onSourceChange} disabled={model.loading || sources.length === 0}>
        <SelectTrigger aria-label={t('contentIndex.sourceLabel')} variant="forensics" size="xs" className="w-44"><SelectValue placeholder={t('contentIndex.selectSource')} /></SelectTrigger>
        <SelectContent>{sources.map((source) => <SelectItem key={source.id} value={source.id}>{source.name}</SelectItem>)}</SelectContent>
      </Select>
      <Button shape="row" variant="forensicsOutline" disabled={!model.available || model.loading} onClick={model.build}>{t(model.coverage?.summary.complete ? 'contentIndex.rebuild' : 'contentIndex.build')}</Button>
      <IconButton label={t('contentIndex.refresh')} disabled={!model.available || model.coverageLoading} onClick={model.refresh}><ActionIcon name="refresh" /></IconButton>
      {model.coverageLoading ? <span role="status">{t('contentIndex.loading')}</span> : null}
      {!model.available ? <span>{t('contentIndex.selectSource')}</span> : null}
      {model.coverage ? <span>{t(model.coverage.stale ? 'contentIndex.stale' : model.coverage.summary.complete ? 'contentIndex.complete' : 'contentIndex.notBuilt', { expected: model.coverage.summary.expected })}</span> : null}
    </div>
    {model.coverage ? <div className="flex flex-wrap gap-3">{STATUSES.map((status) => <span key={status}>{t(`contentIndex.statuses.${status}`)}: {model.coverage!.summary[status]}</span>)}</div> : null}
    {model.coverageError ? <div role="alert">{t('contentIndex.coverageFailed')}</div> : null}
    <EvidenceTaskStatus model={model} />
  </div>;
}
