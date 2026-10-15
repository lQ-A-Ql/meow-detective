import { useTranslation } from 'react-i18next';
import { KeyValueField } from '@/components/data-display';
import { Button } from '@/app/components/ui/button';
import type { NtfsTechnicalFile } from '@/types/files';

export function NtfsStructureFields({ technical, onSelectHex }: { technical: NtfsTechnicalFile; onSelectHex?: (offset: number, length: number) => void }) {
  const { t } = useTranslation();
  return <div className="space-y-2 font-mono text-[10px]">
    <KeyValueField label={t('fileBrowser.inspector.ntfs.sequence')} value={technical.sequenceNumber} />
    <KeyValueField label={t('fileBrowser.inspector.ntfs.flags')} value={technical.flags} />
    <KeyValueField label={t('fileBrowser.inspector.ntfs.parentReference')} value={technical.parentReference ?? '-'} />
    <KeyValueField label={t('fileBrowser.inspector.ntfs.recordSize')} value={technical.recordSize} />
    <Button shape="row" size="xs" variant="forensicsOutline" disabled={!technical.recordRaw.length} onClick={() => onSelectHex?.(0, technical.recordRaw.length)}>{t('fileBrowser.inspector.ntfs.recordHex')}</Button>
    {technical.attributes.map((attribute, index) => <div key={`${attribute.recordOffset}:${index}`} className="space-y-1 border-l border-forensics-divider pl-2">
      <KeyValueField label={t('fileBrowser.inspector.ntfs.attributeType')} value={t(`fileBrowser.inspector.ntfs.attributeTypes.${attribute.attributeType}`, { defaultValue: String(attribute.attributeType) })} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.attributeName')} value={attribute.name ?? '-'} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.instance')} value={attribute.instance} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.residency')} value={t(attribute.nonResident ? 'fileBrowser.inspector.ntfs.nonResident' : 'fileBrowser.inspector.ntfs.resident')} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.allocatedSize')} value={attribute.allocatedSize ?? '-'} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.realSize')} value={attribute.realSize ?? '-'} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.initializedSize')} value={attribute.initializedSize ?? '-'} />
      <KeyValueField label={t('fileBrowser.inspector.ntfs.recordRelativeOffset')} value={attribute.recordOffset} />
      <Button shape="row" size="xs" variant="forensicsOutline" disabled={!technical.recordRaw.length} onClick={() => onSelectHex?.(attribute.recordOffset, attribute.raw.length)}>{t('fileBrowser.inspector.ntfs.attributeHex')}</Button>
    </div>)}
  </div>;
}
