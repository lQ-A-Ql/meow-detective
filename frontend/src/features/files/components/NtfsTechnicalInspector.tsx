import { useTranslation } from 'react-i18next';
import { InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import type { NtfsTechnicalFile } from '@/types/files';

export function NtfsTechnicalInspector({ technical }: { technical?: NtfsTechnicalFile | null }) {
  const { t } = useTranslation();
  if (!technical) return null;
  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.ntfsTechnical')}>
      <div className="space-y-2 font-mono text-[10px] text-forensics-text-secondary">
        <InspectorValue value={`${t('fileBrowser.inspector.ntfs.inode')}: ${technical.inode}`} mono />
        <InspectorValue value={`${t('fileBrowser.inspector.ntfs.recordOffset')}: ${technical.recordOffset}`} mono />
        <InspectorValue value={`${t('fileBrowser.inspector.ntfs.attributes')}: ${technical.attributes.length}`} mono />
        {technical.attributes.flatMap((attribute) => attribute.dataRuns).slice(0, 32).map((run, index) => (
          <div key={`${run.logicalOffset}-${index}`} className="border-l border-forensics-border pl-2">
            <div>{t('fileBrowser.inspector.ntfs.run')} {index + 1}</div>
            <div>LCN {run.absoluteLcn ?? '-'} · {run.clusterCount} clusters</div>
            <div>raw {run.raw.map((value) => value.toString(16).padStart(2, '0')).join(' ')}</div>
          </div>
        ))}
      </div>
    </InspectorSection>
  );
}
