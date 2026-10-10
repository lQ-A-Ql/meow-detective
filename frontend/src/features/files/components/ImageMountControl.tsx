import { HardDriveDownload } from 'lucide-react';
import { Tooltip } from '@/app/components/ui/tooltip';
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
      <Tooltip content={t('fileBrowser.mount.open')}><Button
        type="button"
        variant="forensicsOutline"
        size="iconSm"
        onClick={model.openDialog}
        aria-label={t('fileBrowser.mount.open')}
      >
        <HardDriveDownload size={15} />
        {activeMountCount + activeEmulationCount > 0 ? (
          <Badge variant="secondary">{activeMountCount + activeEmulationCount}</Badge>
        ) : null}
      </Button></Tooltip>
      <ImageMountDialog model={model} />
    </>
  );
}
