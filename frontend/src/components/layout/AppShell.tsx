import { PropsWithChildren } from 'react';
import { BottomDrawerContainer } from '@/features/jobs/containers/BottomDrawerContainer';
import { ActivityBarContainer } from '@/features/shell/containers/ActivityBarContainer';
import { TopBarContainer } from '@/features/shell/containers/TopBarContainer';

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="relative flex h-screen overflow-hidden bg-background font-sans text-[13px] font-normal text-forensics-text selection:bg-forensics-sakura-100 selection:text-forensics-sakura-600">
      <ActivityBarContainer />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <TopBarContainer />
        <div className="flex min-h-0 flex-1 overflow-hidden">{children}</div>
        <BottomDrawerContainer />
      </div>
    </div>
  );
}
