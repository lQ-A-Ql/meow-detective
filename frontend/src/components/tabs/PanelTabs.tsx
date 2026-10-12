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
  variant = 'underline',
  className,
  listClassName,
  triggerClassName,
  onValueChange,
  ...props
}: PanelTabsProps<T>) {
  const isCompact = variant === 'compact';

  return (
    <Tabs className={cn('gap-0', className)} onValueChange={onValueChange} {...props}>
      <TabsList
        className={cn(
          'mb-3 h-9 justify-start overflow-x-auto scrollbar-none bg-transparent p-0',
          isCompact && 'mb-2 h-7',
          listClassName,
        )}
      >
        {tabs.map(({ value, label, icon: Icon }) => (
          <TabsTrigger
            key={value}
            value={value}
            className={cn(
              'flex-none whitespace-nowrap text-[11px]',
              isCompact && 'px-2',
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
