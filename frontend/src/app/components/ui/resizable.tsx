import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { cn } from './utils';
import type { ComponentProps } from 'react';

export function ResizableGroup({ className, ...props }: ComponentProps<typeof PanelGroup>) {
  return <PanelGroup {...props} className={cn('ui-resizable-group', className)} />;
}
export const ResizablePanel = Panel;
export function ResizableHandle({ className, ...props }: ComponentProps<typeof PanelResizeHandle>) {
  return <PanelResizeHandle {...props} className={cn('ui-resizable-handle', className)} />;
}
