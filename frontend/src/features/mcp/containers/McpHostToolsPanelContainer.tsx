import '../mcp-motion.css';
import { useTranslation } from 'react-i18next';
import { useMcpHostModel } from '../use-mcp-host-model';
import { useMcpClipboard } from '../use-mcp-clipboard';
import { McpToolList } from '../components/McpToolList';
import { McpHostToolsPanelView } from '../components/McpHostToolsPanelView';

export function McpHostToolsPanelContainer() {
  const { t } = useTranslation(); const model = useMcpHostModel(); const clipboard = useMcpClipboard(); const settings = model.status?.settings;
  const setDisabled = (name: string, disabled: boolean) => { if (!settings) return; const names = new Set(settings.disabledTools); if (disabled) names.add(name); else names.delete(name); void model.save({ ...settings, disabledTools: [...names] }); };
  return <McpHostToolsPanelView t={t} status={model.status} loading={model.loading} copied={clipboard.copiedValue === model.status?.endpoint && Boolean(clipboard.copiedValue)} copyError={clipboard.copyError}
    onEnable={(enabled) => { if (settings) void model.save({ ...settings, enabled }); }} onCopy={() => { if (model.status) void clipboard.copy(model.status.endpoint); }} onRetry={() => { if (settings) void model.save(settings); }}
    toolList={<McpToolList title={t('mcpUi.host.title')} tools={model.tools} loading={model.loading} error={model.error instanceof Error ? model.error.message : model.status?.lastError} onRefresh={model.refresh} onTestTool={model.callTool} disabledTools={settings?.disabledTools ?? []} toolAccess={settings?.enabled ? 'allowAll' : 'disabled'} onToggleTool={setDisabled} onToggleAll={(disabled) => { if (settings) void model.save({ ...settings, disabledTools: disabled ? model.tools.map((tool) => tool.name) : [] }); }} />} />;
}
export const McpHostToolsPanel = McpHostToolsPanelContainer;
