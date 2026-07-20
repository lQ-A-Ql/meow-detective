import type { ComponentProps, ElementType, ReactNode } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { cn } from '@/app/components/ui/utils';

export interface PanelTabItem<T extends string = string> {
  value: T;
  label: ReactNode;
  icon?: ElementType<{ size?: string | number; className?: string }>;
}

interface PanelTabsProps<T extends string = string>
  extends Omit<ComponentProps<typeof Tabs>, 'children'> {
  tabs: PanelTabItem<T>[];
  children: ReactNode;
  variant?: 'pills' | 'underline' | 'compact';
  listClassName?: string;
  triggerClassName?: string;
}

export function PanelTabs<T extends string = string>({
  tabs,
  children,
  variant = 'pills',
  className,
  listClassName,
  triggerClassName,
  onValueChange,
  ...props
}: PanelTabsProps<T>) {
  const isUnderline = variant === 'underline';
  const isCompact = variant === 'compact';

  return (
    <Tabs className={cn('gap-0', className)} onValueChange={onValueChange} {...props}>
      <TabsList
        className={cn(
          isUnderline
            ? 'h-auto justify-start overflow-x-auto rounded-none border-b border-forensics-border bg-forensics-panel p-0'
            : 'mb-3 h-auto flex-wrap justify-start rounded-none bg-transparent p-0',
          isCompact && 'mb-2 h-7 rounded-none bg-forensics-panel-strong p-0',
          listClassName,
        )}
      >
        {tabs.map(({ value, label, icon: Icon }) => (
          <TabsTrigger
            key={value}
            value={value}
            onClick={() => onValueChange?.(value)}
            className={cn(
              isUnderline
                ? 'h-auto flex-none whitespace-nowrap rounded-none border-b border-transparent px-4 py-2 text-[11px] font-light text-forensics-muted data-[state=active]:border-forensics-sakura-500 data-[state=active]:text-forensics-text'
                : 'h-7 flex-none rounded-none border border-transparent px-2 text-[11px] data-[state=active]:border-forensics-border-strong data-[state=active]:bg-forensics-highlight data-[state=active]:text-forensics-text',
              isCompact && 'h-5 rounded-none px-2 text-[11px]',
              triggerClassName,
            )}
          >
            {Icon ? <Icon size={12} /> : null}
            {label}
          </TabsTrigger>
        ))}
      </TabsList>
      {children}
    </Tabs>
  );
}

export { TabsContent };
