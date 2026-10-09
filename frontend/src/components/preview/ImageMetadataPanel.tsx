import { useTranslation } from 'react-i18next';
import type { ImageMetadata } from '@/types/models';

interface ImageMetadataPanelProps {
  metadata: ImageMetadata | null | undefined;
}

const fields = [
  ['format', 'format'],
  ['width', 'width'],
  ['height', 'height'],
  ['orientation', 'orientation'],
  ['make', 'make'],
  ['model', 'model'],
  ['software', 'software'],
  ['dateTimeOriginal', 'dateTimeOriginal'],
  ['createDate', 'createDate'],
  ['modifyDate', 'modifyDate'],
  ['lensModel', 'lensModel'],
  ['latitude', 'latitude'],
  ['longitude', 'longitude'],
  ['altitude', 'altitude'],
  ['gpsDateTime', 'gpsDateTime'],
] as const;

export function ImageMetadataPanel({ metadata }: ImageMetadataPanelProps) {
  const { t } = useTranslation();
  if (!metadata) {
    return <div className="text-forensics-muted-light">{t('fileBrowser.preview.imageMetadata.loading')}</div>;
  }
  const visible = fields.filter(([key]) => metadata[key] !== undefined && metadata[key] !== null);
  if (metadata.status !== 'present' || visible.length === 0) {
    return (
      <div className="space-y-1 text-[11px] text-forensics-muted">
        <div>{t('fileBrowser.preview.imageMetadata.status')}</div>
        <div className="font-mono text-forensics-text-secondary">{t(`fileBrowser.preview.imageMetadata.states.${metadata.status}`)}</div>
        <div>{t(`fileBrowser.preview.imageMetadata.${metadata.status}`)}</div>
      </div>
    );
  }
  return (
    <div className="space-y-1 font-mono text-[11px] text-forensics-text-secondary">
      <div className="mb-2 text-[10px] text-forensics-muted">{t('fileBrowser.preview.imageMetadata.status')}: {t(`fileBrowser.preview.imageMetadata.states.${metadata.status}`)}</div>
      {visible.map(([key, label]) => (
        <div key={key} className="grid grid-cols-[minmax(100px,auto)_1fr] gap-2">
          <span className="text-forensics-muted-light">{t(`fileBrowser.preview.imageMetadata.fields.${label}`)}</span>
          <span className="break-all">{String(metadata[key])}</span>
        </div>
      ))}
    </div>
  );
}
