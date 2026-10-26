import type { McpHostTool } from '@/types/mcpHost';

export function toggleQueryTools(disabledTools: string[], tools: McpHostTool[], disabled: boolean): string[] {
  const names = new Set(disabledTools);
  for (const tool of tools) {
    if (!tool.readOnly) continue;
    if (disabled) names.add(tool.name);
    else names.delete(tool.name);
  }
  return [...names];
}
