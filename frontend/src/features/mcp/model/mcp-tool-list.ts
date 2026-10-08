import type { JsonSchema, McpTool } from '@/lib/api/mcp-protocol';
export interface McpToolListProps { tools: McpTool[]; loading: boolean; error?: string | null; onRefresh: () => void; onTestTool: (toolName: string, args: unknown) => Promise<{ success: boolean; data?: unknown; error?: string }>; disabledTools?: string[]; onToggleTool?: (toolName: string, disabled: boolean) => void; toolAccess?: 'allowAll' | 'disabled' | 'allowList'; onEnableAll?: () => void; onToggleAll?: (disabled: boolean) => void; title?: string }
export type { JsonSchema, McpTool };
