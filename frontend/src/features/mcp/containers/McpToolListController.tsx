import { useTranslation } from 'react-i18next';
import { useMcpToolModel } from '../use-mcp-tool-model';
import { McpToolListView } from '../components/McpToolListView';
import type { McpToolListProps } from '../model/mcp-tool-list';
export function McpToolListController(props: McpToolListProps) { const { t } = useTranslation(); const model = useMcpToolModel(props.tools, props.loading, props.onTestTool, props.disabledTools ?? [], props.toolAccess ?? 'allowAll'); return <McpToolListView {...props} t={t} {...model} onQuery={model.setQuery} onArguments={(value) => { model.setArgumentsText(value); model.setArgumentError(undefined); }} />; }
