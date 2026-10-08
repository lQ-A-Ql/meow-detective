import { useEffect, useMemo, useState } from 'react';
import type { JsonSchema, McpTool } from '@/lib/api/mcp-protocol';

export interface McpToolResult { toolName: string; success: boolean; data?: unknown; error?: string }

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

export function formatMcpResult(value: unknown): string {
  if (typeof value === 'string') return value;
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}

export function useMcpToolModel(tools: McpTool[], loading: boolean, onTestTool: (name: string, args: unknown) => Promise<{ success: boolean; data?: unknown; error?: string }>, disabledTools: string[], toolAccess: 'allowAll' | 'disabled' | 'allowList') {
  const [query, setQuery] = useState('');
  const [selectedToolName, setSelectedToolName] = useState<string>();
  const [argumentsText, setArgumentsText] = useState('');
  const [argumentError, setArgumentError] = useState<string>();
  const [testingTool, setTestingTool] = useState<string>();
  const [testResult, setTestResult] = useState<McpToolResult>();
  const filteredTools = useMemo(() => { const value = query.trim().toLowerCase(); return value ? tools.filter((tool) => `${tool.name} ${tool.description}`.toLowerCase().includes(value)) : tools; }, [query, tools]);
  const selectedTool = tools.find((tool) => tool.name === selectedToolName) ?? filteredTools[0];
  const isToolDisabled = (name: string) => toolAccess === 'disabled' || disabledTools.some((item) => item.toLowerCase() === name.toLowerCase());
  const disabledCount = tools.filter((tool) => isToolDisabled(tool.name)).length;
  useEffect(() => { if (selectedToolName && !tools.some((tool) => tool.name === selectedToolName)) { setSelectedToolName(undefined); setArgumentsText(''); setArgumentError(undefined); } }, [selectedToolName, tools]);
  const selectTool = (tool: McpTool) => { setSelectedToolName(tool.name); setArgumentsText(formatMcpArguments(tool.inputSchema)); setArgumentError(undefined); setTestResult(undefined); };
  const runTool = async (tool: McpTool, rawArguments = tool.name === selectedToolName ? argumentsText : formatMcpArguments(tool.inputSchema)) => {
    if (testingTool || loading || isToolDisabled(tool.name)) return;
    let args: unknown;
    try { args = JSON.parse(rawArguments || '{}'); } catch { setSelectedToolName(tool.name); setArgumentsText(rawArguments); setArgumentError('mcpUi.tools.invalidJson'); return; }
    setSelectedToolName(tool.name); setArgumentsText(JSON.stringify(args, null, 2)); setArgumentError(undefined); setTestingTool(tool.name); setTestResult(undefined);
    try { setTestResult({ toolName: tool.name, ...(await onTestTool(tool.name, args)) }); } catch (cause) { setTestResult({ toolName: tool.name, success: false, error: cause instanceof Error ? cause.message : 'mcpUi.tools.callFailed' }); } finally { setTestingTool(undefined); }
  };
  return { query, setQuery, filteredTools, selectedTool, argumentsText, setArgumentsText, argumentError, setArgumentError, testingTool, testResult, disabledCount, isToolDisabled, selectTool, runTool };
}
