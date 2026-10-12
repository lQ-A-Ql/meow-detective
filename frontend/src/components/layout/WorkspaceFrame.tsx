import type { HTMLAttributes } from 'react';
import { cn } from '@/app/components/ui/utils';

export function WorkspaceFrame({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div data-slot="workspace-frame" className={cn('workspace-frame flex h-full min-h-0 w-full min-w-0 flex-1 overflow-hidden', className)} {...props} />;
}
