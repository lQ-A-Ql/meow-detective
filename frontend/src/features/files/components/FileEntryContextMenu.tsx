import type { ReactElement } from 'react';
import { useTranslation } from 'react-i18next';
import { Download, HardDrive } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/app/components/ui/context-menu';
import type { FileEntryRow } from '@/types/models';

export interface FileEntryContextMenuProps {
  file: FileEntryRow;
  children: ReactElement;
  onExtractFile: (file: FileEntryRow) => void;
  onOpenNested?: (file: FileEntryRow) => void;
}

export function FileEntryContextMenu({
  file,
  children,
  onExtractFile,
  onOpenNested,
}: FileEntryContextMenuProps) {
  const { t } = useTranslation();
  if (file.entryType !== 'file') {
    return children;
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent aria-label={t('fileBrowser.contextMenu.label', { name: file.name })}>
        <ContextMenuItem onSelect={() => onExtractFile(file)}>
          <Download />
          {t('fileBrowser.inspector.extract.button')}
        </ContextMenuItem>
        {onOpenNested && /\.vhd$/i.test(file.name) ? <ContextMenuItem onSelect={() => onOpenNested(file)}><HardDrive />{t('fileBrowser.contextMenu.openNested')}</ContextMenuItem> : null}
      </ContextMenuContent>
    </ContextMenu>
  );
}
