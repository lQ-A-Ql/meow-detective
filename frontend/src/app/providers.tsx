import { QueryClientProvider } from '@tanstack/react-query';
import type { PropsWithChildren } from 'react';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/i18n';
import { useWorkbenchQueryClient } from '@/features/query-runtime';

export function AppProviders({ children }: PropsWithChildren) {
  const queryClient = useWorkbenchQueryClient();

  return (
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    </I18nextProvider>
  );
}
