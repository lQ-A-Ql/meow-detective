import { useState } from 'react';
import type { McpPermissionProfile } from '@/lib/api/mcp-protocol';
import { defaultPermissions, formatError } from '@/stores/mcp-error-utils';

export function useMcpServerDialogModel(testConnection: McpServerDialogTest) {
  const [name, setName] = useState(''); const [transportType, setTransportType] = useState<'sse' | 'stdio'>('sse'); const [url, setUrl] = useState('http://localhost:3001'); const [command, setCommand] = useState(''); const [argsStr, setArgsStr] = useState(''); const [enabled, setEnabled] = useState(true); const [autoConnect, setAutoConnect] = useState(false); const [permissions, setPermissions] = useState<McpPermissionProfile>(() => defaultPermissions());
  const [testing, setTesting] = useState(false); const [testResult, setTestResult] = useState<{ success: boolean; error?: string } | null>(null); const [saving, setSaving] = useState(false); const [error, setError] = useState<string | null>(null);
  const updatePermission = <K extends keyof McpPermissionProfile>(key: K, value: McpPermissionProfile[K]) => setPermissions((current) => ({ ...current, [key]: value }));
  const test = async () => { setTesting(true); setTestResult(null); try { setTestResult(await testConnection(transportType, url, command, splitArgs(argsStr), permissions)); } catch (cause) { setTestResult({ success: false, error: formatError(cause) }); } finally { setTesting(false); } };
  const save = async (onAdd: McpServerDialogAdd, onClose: () => void) => { const validation = !name.trim() ? 'mcpUi.dialog.requiredName' : transportType === 'sse' && !url.trim() ? 'mcpUi.dialog.requiredUrl' : transportType === 'stdio' && !command.trim() ? 'mcpUi.dialog.requiredCommand' : undefined; if (validation) { setError(validation); return; } setSaving(true); setError(null); try { await onAdd({ name: name.trim(), transportType, url: transportType === 'sse' ? url : undefined, command: transportType === 'stdio' ? command : undefined, args: splitArgs(argsStr), enabled, autoConnect, permissions }); onClose(); } catch (cause) { setError(formatError(cause)); } finally { setSaving(false); } };
  return { name, setName, transportType, setTransportType, url, setUrl, command, setCommand, argsStr, setArgsStr, enabled, setEnabled, autoConnect, setAutoConnect, permissions, updatePermission, testing, testResult, saving, error, test, save, setError };
}
function splitArgs(value: string) { return value ? value.split(' ').filter(Boolean) : undefined; }
export type McpServerDialogAdd = (server: { name: string; transportType: 'sse' | 'stdio'; url?: string; command?: string; args?: string[]; enabled: boolean; autoConnect: boolean; permissions: McpPermissionProfile }) => Promise<void>;
export type McpServerDialogTest = (transportType: string, url?: string, command?: string, args?: string[], permissions?: McpPermissionProfile) => Promise<{ success: boolean; error?: string }>;
