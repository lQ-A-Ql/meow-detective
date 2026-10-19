import type { PropsWithChildren } from 'react';
import { AppShell } from '@/components/layout/AppShell';
import { useShellModel } from '../use-shell-model';
import { useUpdateStartup } from '@/features/app-update/use-update-model';

export function ShellContainer({ children }: PropsWithChildren) {
  useUpdateStartup();
  const model = useShellModel();
  return <AppShell {...model}>{children}</AppShell>;
}
