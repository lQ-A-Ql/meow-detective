import { PropsWithChildren } from 'react';
import { TopBar } from '@/app/components/TopBar';
import { BottomDrawer } from '@/app/components/BottomDrawer';

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-white text-[#111] font-sans text-[13px] selection:bg-[#222] selection:text-white">
      <TopBar />
      <div className="flex min-h-0 flex-1 overflow-hidden">{children}</div>
      <BottomDrawer />
    </div>
  );
}
