import type { RegistryExtractionSummary, RegistryStructuredSummary } from '@/types/models';
import { useRegistryPanelModel } from '../hooks/use-registry-panel-model';
import { RegistryExtractionPanelView } from '../components/panels/RegistryExtractionPanelView';

export function RegistryExtractionPanelContainer({ summary, structured, dataSourceId }: { summary?: RegistryExtractionSummary; structured?: RegistryStructuredSummary; dataSourceId?: string }) {
  const model = useRegistryPanelModel(summary, structured);
  return <RegistryExtractionPanelView model={model} dataSourceId={dataSourceId} />;
}
