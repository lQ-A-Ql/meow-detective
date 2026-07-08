import { ReactNode } from 'react';
import { ViewerTabFrame } from '@/components/tabs/ViewerTabFrame';

interface ViewerTabsProps {
  value: string;
  onValueChange: (value: string) => void;
  tabs: Array<{ value: string; label: ReactNode; content: ReactNode; contentClassName?: string }>;
}

export function ViewerTabs({ value, onValueChange, tabs }: ViewerTabsProps) {
  return <ViewerTabFrame value={value} onValueChange={onValueChange} tabs={tabs} />;
}
