import { useTranslation } from 'react-i18next';
import { InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import type { NtfsTechnicalFile } from '@/types/files';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import type { ntfsInspectorRuns } from '../ntfs-inspector-runs';
import { NtfsStructureFields } from './NtfsStructureFields';
import { NtfsStructureHex } from './NtfsStructureHex';
import type { useNtfsTechnicalFile } from '../hooks/use-ntfs-technical-file';

export function NtfsTechnicalInspector({ technical, eligible, loading, errorText, unsupported, runs, onRetry, navigation }: {
  technical?: NtfsTechnicalFile;
  eligible?: boolean;
  loading?: boolean;
  errorText?: string;
  unsupported?: boolean;
  runs?: ReturnType<typeof ntfsInspectorRuns>;
  onRetry?: () => void;
  navigation?: Pick<ReturnType<typeof useNtfsTechnicalFile>, 'hex' | 'selectHex' | 'setDecoded' | 'previousRuns' | 'nextRuns'>;
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
        <NtfsStructureFields technical={technical} onSelectHex={navigation?.selectHex} />
        {runs?.rows.map((run, index) => (
          <div key={`${run.logicalOffset}-${index}`} className="border-l border-forensics-divider pl-2">
            <div>{t('fileBrowser.inspector.ntfs.run')} {(runs?.page ?? 0) * 32 + index + 1}</div>
            <div>{t('fileBrowser.inspector.ntfs.runSummary', { lcn: run.absoluteLcn ?? '-', clusters: run.clusterCount })}</div>
            <div>{t('fileBrowser.inspector.ntfs.raw', { value: run.rawHex })}</div>
            <div>{t('fileBrowser.inspector.ntfs.runFields', { header: run.header, lengthBytes: run.lengthFieldSize, offsetBytes: run.offsetFieldSize, relativeLcn: run.relativeLcn ?? '-', logicalOffset: run.logicalOffset, physicalOffset: run.physicalOffset ?? '-' })}</div>
            {navigation ? <Button size="xs" variant="forensicsOutline" disabled={!technical.recordRaw.length} onClick={() => navigation.selectHex(run.recordOffset, run.rawLength)}>{t('fileBrowser.inspector.ntfs.runHex')}</Button> : null}
          </div>
        ))}
        {runs && runs.total > runs.rows.length ? <InspectorValue value={t('fileBrowser.inspector.ntfs.truncated', { shown: runs.rows.length, total: runs.total })} /> : null}
        {navigation && runs && runs.totalPages > 1 ? <div className="flex items-center gap-2">
          <Button size="xs" variant="forensicsOutline" disabled={runs.page === 0} onClick={navigation.previousRuns}>{t('fileBrowser.inspector.ntfs.previousRuns')}</Button>
          <InspectorValue value={t('fileBrowser.inspector.ntfs.runPage', { page: runs.page + 1, pages: runs.totalPages })} />
          <Button size="xs" variant="forensicsOutline" disabled={runs.page + 1 >= runs.totalPages} onClick={navigation.nextRuns}>{t('fileBrowser.inspector.ntfs.nextRuns')}</Button>
        </div> : null}
        <NtfsStructureHex hex={navigation?.hex} sourceRanges={technical.sourceRanges} onDecodedChange={navigation?.setDecoded} />
      </div> : null}
    </InspectorSection>
  );
}
