import type { McpPermissionProfile } from '@/lib/api/mcp';

export function defaultPermissions(): McpPermissionProfile {
  return {
    resourceAccess: 'readOnly',
    toolAccess: 'disabled',
    promptAccess: 'readOnly',
    networkPolicy: 'localhostOnly',
    allowedTools: [],
    allowedCommands: [],
  };
}

function hasMessage(err: unknown): err is { message: unknown } {
  return typeof err === 'object' && err !== null && 'message' in err;
}

export function formatError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (hasMessage(err)) {
    return String(err.message);
  }
  return '未知错误';
}
