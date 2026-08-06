import { HardDriveDownload } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import type { ImageMountModel } from '@/features/files/hooks/use-image-mount-model';
import { ImageMountDialog } from '@/features/files/components/ImageMountDialog';

interface ImageMountControlProps {
  model: ImageMountModel;
}

export function ImageMountControl({ model }: ImageMountControlProps) {
  const { t } = useTranslation();
  const activeMountCount = model.mounts.filter((mount) => mount.state === 'mounted').length;
  const activeEmulationCount = model.emulationSessions.filter((session) => (
    session.state !== 'released' && session.state !== 'failedCleanupPending'
  )).length;

  return (
    <>
      <Button
        type="button"
        variant="forensicsOutline"
        size="xs"
        className="shrink-0 gap-1.5"
        onClick={model.openDialog}
        aria-label={t('fileBrowser.mount.open')}
      >
        <HardDriveDownload size={13} />
        <span>{t('fileBrowser.mount.open')}</span>
        {activeMountCount + activeEmulationCount > 0 ? (
          <Badge variant="secondary">{activeMountCount + activeEmulationCount}</Badge>
        ) : null}
      </Button>
      <ImageMountDialog model={model} />
    </>
  );
}
