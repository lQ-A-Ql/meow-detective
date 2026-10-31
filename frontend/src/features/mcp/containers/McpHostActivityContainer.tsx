import { useTranslation } from 'react-i18next';
import { useMcpActivityModel } from '../use-mcp-activity-model';
import { McpHostActivityView } from '../components/McpHostActivityView';

export function McpHostActivityContainer({ enabled }: { enabled: boolean }) {
  const { t } = useTranslation();
  const model = useMcpActivityModel(enabled);
  return <McpHostActivityView t={t} {...model} enabled={model.available} onRefresh={model.refresh} />;
}
