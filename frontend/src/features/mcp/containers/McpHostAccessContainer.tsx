import { useTranslation } from 'react-i18next';
import { errorMessage } from '@/lib/errors';
import { useMcpAccessModel } from '../use-mcp-access-model';
import { useMcpClipboard } from '../use-mcp-clipboard';
import { McpHostAccessView } from '../components/McpHostAccessView';

export function McpHostAccessContainer() {
  const { t, i18n } = useTranslation();
  const model = useMcpAccessModel();
  const clipboard = useMcpClipboard();
  const clients = model.clients.map((client) => ({ ...client,
    expiry: new Date(client.expiresAt * 1000).toLocaleString(i18n.language),
  }));
  return <McpHostAccessView t={t} {...model} clients={clients}
    error={model.error ? errorMessage(model.error) : undefined}
    onCopy={() => { if (model.created) void clipboard.copy(model.created.token); }}
    copied={Boolean(model.created && clipboard.copiedValue === model.created.token)} copyError={clipboard.copyError} />;
}
