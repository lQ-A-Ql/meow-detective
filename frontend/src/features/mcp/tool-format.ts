import type { JsonSchema } from '@/lib/api/mcp-protocol';

/** Result of a single MCP tool invocation, as returned by the host bridge. */
export interface McpToolResult { toolName: string; success: boolean; data?: unknown; error?: string }

/** Placeholder arguments derived from a tool's JSON schema, pretty-printed for editing. */
export function formatMcpArguments(schema: JsonSchema): string { return JSON.stringify(defaultArguments(schema), null, 2); }

function defaultArguments(schema: JsonSchema): unknown {
  if (schema.type === 'object') {
    const properties = schema.properties ?? {};
    const required = new Set(schema.required ?? Object.keys(properties));
    return Object.fromEntries(Object.entries(properties).filter(([name]) => required.has(name)).map(([name, value]) => [name, defaultArguments(value)]));
  }
  if (schema.type === 'array') return [];
  if (schema.type === 'boolean') return false;
  if (schema.type === 'number' || schema.type === 'integer') return 0;
  return '';
}

/** Renders a tool result payload for display. */
export function formatMcpResult(value: unknown): string {
  if (typeof value === 'string') return value;
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}
