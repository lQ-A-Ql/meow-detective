import type { McpHostTool, McpHostToolCallResult } from '@/types/mcpHost';

export type McpTool = Pick<McpHostTool, 'name' | 'description' | 'inputSchema'>;

export interface McpToolListProps {
  tools: McpTool[];
  loading: boolean;
  error?: string | null;
  onRefresh: () => void;
  onTestTool: (toolName: string, args: unknown) => Promise<McpHostToolCallResult>;
  disabledTools?: string[];
  enabled?: boolean;
  onToggleTool?: (toolName: string, disabled: boolean) => void;
  onToggleAll?: (disabled: boolean) => void;
  title?: string;
}
