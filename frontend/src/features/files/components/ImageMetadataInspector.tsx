import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import type { ApiErrorDto, ImageMetadata } from '@/types/models';

interface ImageMetadataInspectorProps {
  metadata?: ImageMetadata | null;
  loading: boolean;
  error?: ApiErrorDto | null;
  onRetry?: () => void;
}

function MetadataField({ label, value }: { label: string; value?: string | number }) {
  if (value === undefined || value === null || value === '') return null;
  return (
    <div className="grid grid-cols-[92px_1fr] gap-2 font-mono text-[10px]">
      <div className="text-forensics-muted-light">{label}</div>
      <div className="break-all text-forensics-text-secondary">{String(value)}</div>
    </div>
  );
}

export function ImageMetadataInspector({ metadata, loading, error, onRetry }: ImageMetadataInspectorProps) {
  const { t } = useTranslation();
  const status = metadata?.status;
  const statusLabel = status ? t(`fileBrowser.inspector.imageMetadata.statuses.${status}`) : undefined;

  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.imageMetadata')}>
      {loading ? <div className="text-[11px] text-forensics-muted">{t('fileBrowser.inspector.imageMetadata.loading')}</div> : null}
      {!loading && error ? (
        <div className="space-y-2">
          <div className="text-[11px] text-forensics-error-text">{error.message}</div>
          {onRetry ? <Button type="button" size="xs" variant="forensicsOutline" onClick={onRetry}>{t('fileBrowser.inspector.imageMetadata.retry')}</Button> : null}
        </div>
      ) : null}
      {!loading && !error && metadata ? (
        <div className="space-y-2">
          <InspectorValue value={statusLabel ?? '-'} mono strong />
          {metadata.status === 'present' ? (
            <div className="space-y-1.5">
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.format')} value={metadata.format} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.dimensions')} value={metadata.width !== undefined && metadata.height !== undefined ? `${metadata.width} × ${metadata.height}` : undefined} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.orientation')} value={metadata.orientation} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.make')} value={metadata.make} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.model')} value={metadata.model} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.software')} value={metadata.software} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.dateTimeOriginal')} value={metadata.dateTimeOriginal} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.createDate')} value={metadata.createDate} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.modifyDate')} value={metadata.modifyDate} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.lensModel')} value={metadata.lensModel} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.latitude')} value={metadata.latitude} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.longitude')} value={metadata.longitude} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.altitude')} value={metadata.altitude} />
              <MetadataField label={t('fileBrowser.inspector.imageMetadata.gpsDateTime')} value={metadata.gpsDateTime} />
            </div>
          ) : (
            <div className="text-[11px] leading-5 text-forensics-muted">{t(`fileBrowser.inspector.imageMetadata.statuses.${metadata.status}`)}</div>
          )}
        </div>
      ) : null}
    </InspectorSection>
  );
}
