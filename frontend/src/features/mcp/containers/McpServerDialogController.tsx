import { useTranslation } from 'react-i18next';
import { useMcpServerDialogModel } from '../use-mcp-server-dialog-model';
import { McpServerDialogView } from '../components/McpServerDialogView';
import type { McpServerDialogProps } from '../model/mcp-server-dialog';
export type ReturnTypeModel = ReturnType<typeof useMcpServerDialogModel>;
export function McpServerDialogController(props: McpServerDialogProps) { const { t } = useTranslation(); const model = useMcpServerDialogModel(props.testConnection); return <McpServerDialogView t={t} model={model} props={props} />; }
