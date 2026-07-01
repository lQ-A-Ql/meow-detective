import { PropsWithChildren } from 'react';
import { TopBar } from './TopBar';
import { BottomDrawer } from './BottomDrawer';

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-forensics-surface font-sans text-[13px] text-forensics-text selection:bg-forensics-text-secondary selection:text-white">
      <TopBar />
      <div className="flex min-h-0 flex-1 overflow-hidden">{children}</div>
      <BottomDrawer />
    </div>
  );
}
