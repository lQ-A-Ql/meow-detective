import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import type { RegistryBrowserContext } from '../types';

export function useNativeRegistryContext(dataSourceId?: string): RegistryBrowserContext | undefined {
  const currentCase = useCurrentCase();
  const sources = useDataSources();
  const source = sources.data?.find((item) => item.id === dataSourceId);
  if (!currentCase.data || !source || source.platform !== 'windows' || source.importState !== 'ready') return undefined;
  const catalog = source.processing?.phases.find((phase) => phase.phase === 'catalog');
  return { caseId: currentCase.data.id, dataSourceId: source.id,
    sourceRevision: [source.importedAt, source.schemaVersion, catalog?.updatedAt, sources.dataUpdatedAt].join(':') };
}
