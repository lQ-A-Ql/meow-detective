import type { PropsWithChildren } from 'react';
import { AppShell } from '@/components/layout/AppShell';
import { useShellModel } from '../use-shell-model';

export function ShellContainer({ children }: PropsWithChildren) {
  const model = useShellModel();
  return <AppShell {...model}>{children}</AppShell>;
}
