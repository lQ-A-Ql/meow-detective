import { useEffect, useMemo, useState } from 'react';
import type { McpTool } from './model/mcp-tool-list';
export { formatMcpArguments, formatMcpResult, type McpToolResult } from './tool-format';
import { formatMcpArguments } from './tool-format';
import type { McpToolResult as McpToolResultType } from './tool-format';

export function useMcpToolModel(tools: McpTool[], loading: boolean, onTestTool: (name: string, args: unknown) => Promise<{ success: boolean; data?: unknown; error?: string }>, disabledTools: string[], enabled: boolean) {
  const [query, setQuery] = useState('');
  const [selectedToolName, setSelectedToolName] = useState<string>();
  const [argumentsText, setArgumentsText] = useState('');
  const [argumentError, setArgumentError] = useState<string>();
  const [testingTool, setTestingTool] = useState<string>();
  const [testResult, setTestResult] = useState<McpToolResultType>();
  const filteredTools = useMemo(() => { const value = query.trim().toLowerCase(); return value ? tools.filter((tool) => `${tool.name} ${tool.description}`.toLowerCase().includes(value)) : tools; }, [query, tools]);
  const selectedTool = tools.find((tool) => tool.name === selectedToolName) ?? filteredTools[0];
  const isToolDisabled = (name: string) => !enabled || disabledTools.some((item) => item.toLowerCase() === name.toLowerCase());
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
