import type { PropsWithChildren } from 'react';
import { BottomDrawerContainer } from '@/features/jobs/containers/BottomDrawerContainer';
import { ActivityBarContainer } from '@/features/shell/containers/ActivityBarContainer';
import { CommandPalette, type CommandPaletteItem } from '@/app/components/CommandPalette';
import { Toaster } from '@/app/components/ui/toast';

export function AppShell({ children, isCaseEntry, commands }: PropsWithChildren<{ isCaseEntry: boolean; commands: CommandPaletteItem[] }>) {
  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-forensics-base font-sans text-[13px] font-normal text-forensics-ink selection:bg-forensics-red-tint selection:text-forensics-red-deep-ink">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* 侧栏通高：底部横条只在工作区列内展开。 */}
        {isCaseEntry ? null : <ActivityBarContainer />}
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {/* 工作区大卡片：仅左下 20px 圆角；左上与右侧齐边全覆盖。 */}
          <div className={isCaseEntry
            ? 'flex min-w-0 flex-1 flex-col overflow-hidden bg-forensics-paper'
            : 'flex min-w-0 flex-1 flex-col overflow-hidden rounded-bl-shell border border-b-0 border-r-0 border-forensics-line bg-forensics-paper'}>
            {children}
          </div>
          {isCaseEntry ? null : <BottomDrawerContainer />}
        </div>
      </div>
      <CommandPalette commands={commands} />
      <Toaster />
    </div>
  );
}
