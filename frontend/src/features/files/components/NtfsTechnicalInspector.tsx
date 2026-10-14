import { useTranslation } from 'react-i18next';
import { InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import type { NtfsTechnicalFile } from '@/types/files';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import type { ntfsInspectorRuns } from '../ntfs-inspector-runs';

export function NtfsTechnicalInspector({ technical, eligible, loading, errorText, unsupported, runs, onRetry }: {
  technical?: NtfsTechnicalFile;
  eligible?: boolean;
  loading?: boolean;
  errorText?: string;
  unsupported?: boolean;
  runs?: ReturnType<typeof ntfsInspectorRuns>;
  onRetry?: () => void;
}) {
  const { t } = useTranslation();
  if (!eligible) return null;
  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.ntfsTechnical')}>
      {loading ? <InspectorValue value={t('fileBrowser.inspector.ntfs.loading')} /> : null}
      {errorText ? <div className="space-y-2">
        <div role="alert" className="text-[10px] text-forensics-error-text">{unsupported ? t('fileBrowser.inspector.ntfs.unsupported') : errorText}</div>
        {unsupported ? <InspectorValue value={errorText} /> : onRetry ? <Tooltip content={t('fileBrowser.inspector.ntfs.retry')}>
          <Button type="button" size="xs" variant="forensicsOutline" disabled={loading} onClick={onRetry}>{t('fileBrowser.inspector.ntfs.retry')}</Button>
        </Tooltip> : null}
      </div> : null}
      {technical ? <div className="space-y-2 font-mono text-[10px] text-forensics-text-secondary">
        <InspectorValue value={`${t('fileBrowser.inspector.ntfs.inode')}: ${technical.inode}`} mono />
        <InspectorValue value={`${t('fileBrowser.inspector.ntfs.recordOffset')}: ${technical.recordOffset}`} mono />
        <InspectorValue value={`${t('fileBrowser.inspector.ntfs.attributes')}: ${technical.attributes.length}`} mono />
        {runs?.rows.map((run, index) => (
          <div key={`${run.logicalOffset}-${index}`} className="border-l border-forensics-divider pl-2">
            <div>{t('fileBrowser.inspector.ntfs.run')} {index + 1}</div>
            <div>{t('fileBrowser.inspector.ntfs.runSummary', { lcn: run.absoluteLcn ?? '-', clusters: run.clusterCount })}</div>
            <div>{t('fileBrowser.inspector.ntfs.raw', { value: run.rawHex })}</div>
          </div>
        ))}
        {runs && runs.total > runs.rows.length ? <InspectorValue value={t('fileBrowser.inspector.ntfs.truncated', { shown: runs.rows.length, total: runs.total })} /> : null}
      </div> : null}
    </InspectorSection>
  );
}
