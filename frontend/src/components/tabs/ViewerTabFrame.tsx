import type { ReactNode } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { cn } from '@/app/components/ui/utils';

interface ViewerTabFrameProps {
  value: string;
  onValueChange: (value: string) => void;
  tabs: Array<{ value: string; label: ReactNode; content: ReactNode; contentClassName?: string }>;
}

export function ViewerTabFrame({ value, onValueChange, tabs }: ViewerTabFrameProps) {
  return (
    <Tabs value={value} onValueChange={onValueChange} className="flex h-full flex-col gap-0">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] px-0">
        <TabsList className="h-8 w-auto rounded-none bg-transparent p-0">
          {tabs.map((tab) => (
            <TabsTrigger
              key={tab.value}
              value={tab.value}
              className="h-8 rounded-none border-x-0 border-t-0 border-b-2 border-transparent px-4 text-[11px] text-[#666] shadow-none data-[state=active]:border-[#111] data-[state=active]:bg-white data-[state=active]:text-[#111]"
            >
              {tab.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </div>
      {tabs.map((tab) => (
        <TabsContent
          key={tab.value}
          value={tab.value}
          className={cn('min-h-0 flex-1 overflow-auto p-3 text-[11px]', tab.contentClassName)}
        >
          {tab.content}
        </TabsContent>
      ))}
    </Tabs>
  );
}
