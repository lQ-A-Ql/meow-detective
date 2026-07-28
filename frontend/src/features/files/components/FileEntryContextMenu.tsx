import type { ReactElement } from 'react';
import { Download } from 'lucide-react';
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
  onOpenFileMenu: (file: FileEntryRow) => void;
  onExtractFile: (file: FileEntryRow) => void;
}

export function FileEntryContextMenu({
  file,
  children,
  onOpenFileMenu,
  onExtractFile,
}: FileEntryContextMenuProps) {
  if (file.entryType !== 'file') {
    return children;
  }

  return (
    <ContextMenu onOpenChange={(open) => open && onOpenFileMenu(file)}>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent aria-label={`${file.name} 操作菜单`}>
        <ContextMenuItem onSelect={() => onExtractFile(file)}>
          <Download />
          提取文件
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
