import { PropsWithChildren } from 'react';
import { BrandWatermark } from '@/components/brand';
import { BottomDrawerContainer } from '@/features/jobs/containers/BottomDrawerContainer';
import { TopBarContainer } from '@/features/shell/containers/TopBarContainer';

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-background font-serif text-[13px] font-light text-forensics-text selection:bg-forensics-sakura-250 selection:text-forensics-text">
      <TopBarContainer />
      <BrandWatermark
        motif="walking"
        className="pointer-events-none absolute bottom-12 right-8 z-[5] hidden h-56 opacity-[0.035] xl:block"
      />
      <div className="flex min-h-0 flex-1 overflow-hidden">{children}</div>
      <BottomDrawerContainer />
    </div>
  );
}
