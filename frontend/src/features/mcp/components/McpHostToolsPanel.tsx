import { useState } from 'react';
import { getDataSources } from '@/lib/api/case';
import { listPluginModules } from '@/lib/api/analysis';
import type { DataSourceSummary } from '@/types/models';
import type { McpTool } from '@/lib/api/mcp';
import { McpToolList } from './McpToolList';

const HOST_TOOLS: McpTool[] = [
  {
    name: 'forensics.list_data_sources',
    description: '读取当前案件的数据源摘要和分区信息。',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'forensics.list_plugin_modules',
    description: '读取指定数据源的插件模块、家族数量和告警。',
    inputSchema: {
      type: 'object',
      properties: { dataSourceId: { type: 'string', description: '数据源 ID' } },
      required: ['dataSourceId'],
    },
  },
];

export function McpHostToolsPanel() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  const callHostTool = async (toolName: string, args: unknown) => {
    setLoading(true);
    setError(undefined);
    try {
      if (toolName === 'forensics.list_data_sources') {
        const sources = await getDataSources();
        return { success: true, data: sources.map(toSafeDataSourceSummary) };
      }
      if (toolName === 'forensics.list_plugin_modules') {
        const dataSourceId = readDataSourceId(args);
        const modules = await listPluginModules(dataSourceId);
        return { success: true, data: modules };
      }
      return { success: false, error: '未知的内置工具。' };
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      return { success: false, error: message };
    } finally {
      setLoading(false);
    }
  };

  return <McpToolList
    title="内置只读工具"
    tools={HOST_TOOLS}
    loading={loading}
    error={error}
    onRefresh={() => setError(undefined)}
    onTestTool={callHostTool}
  />;
}

function readDataSourceId(value: unknown): string {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('需要填写 dataSourceId。');
  const dataSourceId = (value as { dataSourceId?: unknown }).dataSourceId;
  if (typeof dataSourceId !== 'string' || !dataSourceId.trim()) throw new Error('需要填写有效的 dataSourceId。');
  return dataSourceId.trim();
}

function toSafeDataSourceSummary(source: DataSourceSummary) {
  return {
    id: source.id,
    name: source.name,
    kind: source.kind,
    platform: source.platform,
    importedAt: source.importedAt,
    importState: source.importState,
    fileCount: source.fileCount,
    evidenceSize: source.evidenceSize,
    hashStatus: source.hashStatus,
    processing: source.processing?.state,
    partitions: source.partitions?.map((partition) => ({
      index: partition.index,
      name: partition.name,
      kindLabel: partition.kindLabel,
      status: partition.status,
      filesystem: partition.filesystem,
      length: partition.length,
    })),
  };
}
