import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';

import { Button } from '@/app/components/ui/button';
import { InspectorSection } from '@/components/layout/InspectorPane';
import { ImageMetadataDetails } from '@/components/preview/ImageMetadataDetails';
import type { ApiErrorDto, ImageMetadata } from '@/types/models';

interface ImageMetadataInspectorProps {
  metadata?: ImageMetadata | null;
  fileSize?: number;
  metadataOnly?: boolean;
  loading: boolean;
  error?: ApiErrorDto | null;
  onRetry?: () => void;
}

export function ImageMetadataInspector({
  metadata,
  fileSize,
  metadataOnly,
  loading,
  error,
  onRetry,
}: ImageMetadataInspectorProps) {
  const { t } = useTranslation();

  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.imageMetadata')}>
      {loading ? (
        <div className="text-[11px] text-forensics-muted">{t('fileBrowser.inspector.imageMetadata.loading')}</div>
      ) : null}
      {!loading && error ? (
        <div className="space-y-2">
          <div className="text-[11px] text-forensics-error-text">{error.message}</div>
          {onRetry ? (
            <Tooltip content={t('fileBrowser.inspector.imageMetadata.retry')}>
              <Button
                aria-label={t('fileBrowser.inspector.imageMetadata.retry')}
                type="button"
                size="xs"
                variant="forensicsOutline"
                onClick={onRetry}
              >
                <ActionIcon name="refresh" />
              </Button>
            </Tooltip>
          ) : null}
        </div>
      ) : null}
      {!loading && !error && metadata ? (
        <ImageMetadataDetails metadata={metadata} fileSize={fileSize} metadataOnly={metadataOnly} />
      ) : null}
    </InspectorSection>
  );
}
