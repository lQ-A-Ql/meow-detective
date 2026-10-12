import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { WindowsLaunchPanel } from './WindowsLaunchPanel';
import { LinuxLaunchPanel } from './LinuxLaunchPanel';
import { ErrorNote, ProtectionNote, StartButton } from './launch-shared';
import type { EmulationWorkspaceModel } from '../use-emulation-workspace-model';

interface Props {
  model: EmulationWorkspaceModel;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function EmulationSettingsDialog({ model, open, onOpenChange }: Props) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined}
        className="flex h-[min(680px,calc(100dvh-2rem))] min-h-0 w-[calc(100%-2rem)] flex-col overflow-hidden sm:max-w-[960px]">
        <DialogHeader>
          <DialogTitle>{t('emulationPage.settings.title')}</DialogTitle>
        </DialogHeader>
        <ScrollArea className="min-h-0 flex-1 overflow-hidden" viewportClassName="pr-2">
          {model.selectedSource?.platform === 'LINUX'
            ? <LinuxLaunchPanel model={model} />
            : <WindowsLaunchPanel model={model} />}
          <ErrorNote model={model} />
        </ScrollArea>
        <DialogFooter className="flex-row items-center justify-between gap-4 sm:justify-between">
          <ProtectionNote />
          <div className="flex shrink-0 items-center gap-2">
            <IconButton label={t('common.cancel')} onClick={() => onOpenChange(false)}><ActionIcon name="close" /></IconButton>
            <StartButton model={model} />
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
