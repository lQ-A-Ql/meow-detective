import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { RegistryBrowserContainer } from '@/features/registry-browser/containers/RegistryBrowserContainer';
import { createRegistryBrowserMock, type RegistryPreviewScenario } from './client';
import fixture from './data.json';
import '@/i18n';
import '@/styles/index.css';
import './preview.css';

if (import.meta.env.PROD || import.meta.env.MODE !== 'mock') throw new Error('Registry P0 preview is mock-dev only');
const context = { caseId: fixture.caseId, dataSourceId: fixture.dataSourceId, sourceRevision: 'synthetic-v1' };

function Preview() {
  const { t } = useTranslation();
  const [scenario, setScenario] = useState<RegistryPreviewScenario>('standard');
  const client = useMemo(() => createRegistryBrowserMock(scenario, 35), [scenario]);
  const queryClient = useMemo(() => new QueryClient({ defaultOptions: { queries: { retry: false } } }), [scenario]);
  return <div className="registry-preview">
    <div className="registry-preview-toolbar"><span>{t('registryBrowser.prototype')}</span>
      <Select value={scenario} onValueChange={(value) => setScenario(value as RegistryPreviewScenario)}>
        <SelectTrigger aria-label={t('registryBrowser.scenario')}><SelectValue /></SelectTrigger>
        <SelectContent>{['standard', 'emptySource', 'failedSource'].map((value) => <SelectItem key={value} value={value}>{t(`registryBrowser.scenarios.${value}`)}</SelectItem>)}</SelectContent>
      </Select></div>
    <QueryClientProvider key={scenario} client={queryClient}>
      <RegistryBrowserContainer key={scenario} client={client} context={{ ...context, sourceRevision: 'synthetic-v1' }}
        initialKeyPath="Microsoft\\Windows\\CurrentVersion" />
    </QueryClientProvider>
  </div>;
}

createRoot(document.getElementById('root')!).render(<Preview />);
