import { useTranslation } from 'react-i18next';
import { useMcpClipboard } from '../use-mcp-clipboard';
import { McpResourceListView } from '../components/McpResourceListView';
import type { McpResourceListProps } from '../model/mcp-resource-list';
export function McpResourceListController(props: McpResourceListProps) { const { t } = useTranslation(); const clipboard = useMcpClipboard(); return <McpResourceListView {...props} t={t} copiedUri={clipboard.copiedValue} onCopy={(uri) => void clipboard.copy(uri)} />; }
