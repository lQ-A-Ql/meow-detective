import { PropsWithChildren } from 'react';
import { BottomDrawerContainer } from '@/features/jobs/containers/BottomDrawerContainer';
import { ActivityBarContainer } from '@/features/shell/containers/ActivityBarContainer';

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-background font-sans text-[13px] font-normal text-forensics-text selection:bg-forensics-sakura-100 selection:text-forensics-sakura-600">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <ActivityBarContainer />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">{children}</div>
      </div>
      <BottomDrawerContainer />
    </div>
  );
}
