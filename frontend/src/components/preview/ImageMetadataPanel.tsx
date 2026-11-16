import { useTranslation } from 'react-i18next';
import { ImageMetadataDetails } from '@/components/preview/ImageMetadataDetails';
import type { ImageMetadata } from '@/types/models';

interface ImageMetadataPanelProps {
  metadata: ImageMetadata | null | undefined;
  fileSize?: number;
  metadataOnly?: boolean;
}

export function ImageMetadataPanel({ metadata, fileSize, metadataOnly }: ImageMetadataPanelProps) {
  const { t } = useTranslation();
  if (!metadata) {
    return <div className="text-forensics-muted-light">{t('fileBrowser.preview.imageMetadata.loading')}</div>;
  }
  return <ImageMetadataDetails metadata={metadata} fileSize={fileSize} metadataOnly={metadataOnly} />;
}
