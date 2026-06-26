import { useState, useEffect } from 'react';
import { HardDrive, FolderOpen, Bot, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { useMcpStore } from '@/stores/mcp-store';
import {
  applyTheme,
  formatPathList,
  parsePathList,
  readLocalSettings,
  validatePathList,
  writeLocalSettings,
} from '@/lib/settings';
import { getAppSettings, saveAppSettings } from '@/lib/api/settings';
import { McpServerItem } from '@/components/mcp/McpServerItem';
import { McpServerDialog } from '@/components/mcp/McpServerDialog';
import { McpResourceList } from '@/components/mcp/McpResourceList';
import { McpToolList } from '@/components/mcp/McpToolList';

export function Settings() {
  const [mcpExpanded, setMcpExpanded] = useState(false);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [settings, setSettings] = useState(() => readLocalSettings());
  const [settingsMessage, setSettingsMessage] = useState('');
  const [savingSettings, setSavingSettings] = useState(false);

  const {
    servers,
    selectedServerId,
    loading,
    error,
    loadConfig,
    addServer,
    removeServer,
    connectServer,
    disconnectServer,
    testConnection,
    selectServer,
  } = useMcpStore();

  // Load MCP config on mount
  useEffect(() => {
    loadConfig();
  }, []);

  useEffect(() => {
    let cancelled = false;
    getAppSettings()
      .then((remote) => {
        if (cancelled) {
          return;
        }
        setSettings((current) => ({
          ...current,
          caseRoot: remote.caseRoot,
          imageSearchPaths: formatPathList(remote.imageSearchPaths),
          theme: remote.theme,
          devEventTrace: remote.devEventTrace,
          maxImportWorkers: remote.maxImportWorkers?.toString() ?? '',
          maxAnalysisWorkers: remote.maxAnalysisWorkers?.toString() ?? '',
          importAnalysisMode: remote.importAnalysisMode ?? 'metadataOnly',
        }));
      })
      .catch(() => {
        // Standalone mock/dev mode keeps local settings as the fallback.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    applyTheme(settings.theme);
  }, [settings.theme]);

  const selectedServer = servers.find((s) => s.id === selectedServerId);

  async function saveSettings() {
    if (!settings.caseRoot.trim()) {
      setSettingsMessage('案件目录不能为空。');
      return;
    }
    if (!validatePathList(settings.imageSearchPaths)) {
      setSettingsMessage('镜像搜索路径包含非法字符。');
      return;
    }
    const maxImportWorkers = parseOptionalPositiveInt(settings.maxImportWorkers);
    const maxAnalysisWorkers = parseOptionalPositiveInt(settings.maxAnalysisWorkers);
    if (maxImportWorkers === 0 || maxAnalysisWorkers === 0) {
      setSettingsMessage('Worker 数必须为空或大于 0。');
      return;
    }
    setSavingSettings(true);
    setSettingsMessage('');
    try {
      const saved = await saveAppSettings({
        caseRoot: settings.caseRoot,
        imageSearchPaths: parsePathList(settings.imageSearchPaths),
        theme: settings.theme,
        devEventTrace: settings.devEventTrace,
        maxImportWorkers,
        maxAnalysisWorkers,
        importAnalysisMode: settings.importAnalysisMode,
      });
      const normalized = writeLocalSettings({
        caseRoot: saved.caseRoot,
        imageSearchPaths: formatPathList(saved.imageSearchPaths),
        theme: saved.theme,
        devEventTrace: saved.devEventTrace,
        maxImportWorkers: saved.maxImportWorkers?.toString() ?? '',
        maxAnalysisWorkers: saved.maxAnalysisWorkers?.toString() ?? '',
        importAnalysisMode: saved.importAnalysisMode ?? 'metadataOnly',
      });
      setSettings(normalized);
      setSettingsMessage('设置已保存。');
    } catch (error) {
      const message = error instanceof Error ? error.message : '设置保存失败。';
      setSettingsMessage(message);
    } finally {
      setSavingSettings(false);
    }
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="font-serif text-xl text-[#111] tracking-tight">设置</div>
        <div className="text-[#666] text-[11px] font-mono mt-1">应用配置与数据目录</div>
      </div>

      <div className="p-6 space-y-8">
        {/* 案件目录 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <FolderOpen size={14} className="text-[#888]" />
            <label htmlFor="settings-case-root" className="text-[13px] font-semibold text-[#333]">
              案件目录
            </label>
          </div>
          <input
            id="settings-case-root"
            value={settings.caseRoot}
            onChange={(event) =>
              setSettings((current) => ({ ...current, caseRoot: event.target.value }))
            }
            className="w-full max-w-3xl bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#111]"
          />
          <div className="mt-1 text-[10px] text-[#999]">所有案件数据将存储在此目录下</div>
        </section>

        {/* 镜像搜索路径 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <HardDrive size={14} className="text-[#888]" />
            <label htmlFor="settings-image-search-paths" className="text-[13px] font-semibold text-[#333]">
              镜像搜索路径
            </label>
          </div>
          <input
            id="settings-image-search-paths"
            value={settings.imageSearchPaths}
            onChange={(event) =>
              setSettings((current) => ({ ...current, imageSearchPaths: event.target.value }))
            }
            className="w-full max-w-3xl bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#111]"
          />
          <div className="mt-1 text-[10px] text-[#999]">导入数据源时自动搜索的镜像目录（分号分隔）</div>
        </section>

        <section>
          <div className="flex items-center gap-2 mb-3">
            <span className="text-[13px] font-semibold text-[#333]">导入性能</span>
          </div>
          <div className="grid gap-3 md:grid-cols-2">
            <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
              <span className="block text-[11px] font-semibold text-[#555]">枚举 Worker 上限</span>
              <input
                value={settings.maxImportWorkers}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    maxImportWorkers: event.target.value,
                  }))
                }
                inputMode="numeric"
                placeholder="自动"
                className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
              />
              <span className="mt-1 block text-[10px] text-[#999]">E01/RAW 分区枚举，空值使用自动。</span>
            </label>
            <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
              <span className="block text-[11px] font-semibold text-[#555]">分析 Worker 上限</span>
              <input
                value={settings.maxAnalysisWorkers}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    maxAnalysisWorkers: event.target.value,
                  }))
                }
                inputMode="numeric"
                placeholder="自动"
                className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
              />
              <span className="mt-1 block text-[10px] text-[#999]">artifact/timeline/text 分析池，空值使用逻辑核心。</span>
            </label>
          </div>
          <label className="mt-3 block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
            <span className="block text-[11px] font-semibold text-[#555]">导入分析模式</span>
            <select
              value={settings.importAnalysisMode}
              onChange={(event) =>
                setSettings((current) => ({
                  ...current,
                  importAnalysisMode:
                    event.target.value === 'fullContent'
                      ? 'fullContent'
                      : event.target.value === 'budgetedContent'
                        ? 'budgetedContent'
                        : 'metadataOnly',
                }))
              }
              className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 text-[12px]"
            >
              <option value="metadataOnly">Metadata only (E01/RAW 推荐)</option>
              <option value="budgetedContent">Budgeted content (目录导入推荐)</option>
              <option value="fullContent">Full content (高内存风险)</option>
            </select>
            <span className="mt-1 block text-[10px] text-[#999]">
              E01/RAW 默认只做元数据与时间线；内容读取和全文索引需显式开启预算模式。
            </span>
          </label>
        </section>

        <section>
          <div className="flex items-center gap-2 mb-3">
            <span className="text-[13px] font-semibold text-[#333]">界面与调试</span>
          </div>
          <div className="flex flex-wrap items-center gap-3 text-[12px]">
            <label
              htmlFor="settings-theme"
              className="flex items-center gap-2 border border-[#e0e0e0] bg-[#f8f8f8] px-3 py-2"
            >
              主题
              <select
                id="settings-theme"
                value={settings.theme}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    theme: event.target.value === 'dark' ? 'dark' : 'light',
                  }))
                }
                className="border border-[#ccc] bg-white px-2 py-1"
              >
                <option value="light">浅色</option>
                <option value="dark">深色</option>
              </select>
            </label>
            <label className="flex items-center gap-2 border border-[#e0e0e0] bg-[#f8f8f8] px-3 py-2">
              <input
                type="checkbox"
                checked={settings.devEventTrace}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    devEventTrace: event.target.checked,
                  }))
                }
              />
              事件调试日志
            </label>
            <button
              type="button"
              onClick={saveSettings}
              disabled={savingSettings}
              className="border border-[#111] bg-[#111] px-4 py-2 text-[12px] text-white hover:bg-[#333]"
            >
              {savingSettings ? '保存中...' : '保存设置'}
            </button>
            {settingsMessage ? (
              <span className="text-[11px] text-[#666]">{settingsMessage}</span>
            ) : null}
          </div>
        </section>

        {/* MCP 配置 */}
        <section>
          <div
            className="flex items-center gap-2 mb-3 cursor-pointer select-none"
            onClick={() => setMcpExpanded(!mcpExpanded)}
          >
            <Bot size={14} className="text-[#888]" />
            <span className="text-[13px] font-semibold text-[#333]">AI 助手 (MCP)</span>
            {mcpExpanded ? (
              <ChevronDown size={14} className="text-[#888]" />
            ) : (
              <ChevronRight size={14} className="text-[#888]" />
            )}
            {loading && (
              <span className="text-[10px] text-blue-500">加载中...</span>
            )}
          </div>

          {mcpExpanded && (
            <div className="space-y-4">
              {/* 服务器列表 */}
              <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3">
                <div className="text-[11px] font-semibold text-[#666] mb-2">
                  MCP 服务器连接
                </div>
                <div className="space-y-1">
                  {servers.length === 0 ? (
                    <div className="text-[11px] text-gray-500 py-2">暂无服务器</div>
                  ) : (
                    servers.map((server) => (
                      <McpServerItem
                        key={server.id}
                        server={server}
                        isSelected={server.id === selectedServerId}
                        onConnect={() => connectServer(server.id)}
                        onDisconnect={() => disconnectServer(server.id)}
                        onRemove={() => removeServer(server.id)}
                        onSelect={() => selectServer(server.id)}
                      />
                    ))
                  )}
                </div>
                <button
                  onClick={() => setShowAddDialog(true)}
                  className="mt-2 flex items-center gap-1 text-[11px] text-blue-600 hover:text-blue-800 transition-colors"
                >
                  <Plus size={12} />
                  添加服务器
                </button>
              </div>

              {/* Resources 和 Tools */}
              {selectedServer && (
                <div className="grid grid-cols-2 gap-4">
                  <McpResourceList serverId={selectedServer.id} />
                  <McpToolList serverId={selectedServer.id} />
                </div>
              )}

              {/* 连接状态 */}
              <div className="text-[11px] text-[#666]">
                连接状态:{' '}
                <span className="font-medium">
                  {servers.filter((s) => s.connected).length}
                </span>{' '}
                个服务器已连接
              </div>

              {/* Error */}
              {error && (
                <div className="p-3 rounded text-[12px] bg-red-50 text-red-700 border border-red-200">
                  {error}
                </div>
              )}
            </div>
          )}
        </section>

        {/* 系统信息 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <span className="text-[13px] font-semibold text-[#333]">系统信息</span>
          </div>
          <div className="space-y-2 text-[12px] font-mono text-[#666]">
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>版本</span>
              <span>Forensics Workbench 0.1.0</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>平台</span>
              <span>{navigator.platform || 'Windows'}</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>数据库</span>
              <span>SQLite (每个案件独立)</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>搜索引擎</span>
              <span>Tantivy (全文索引)</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>MCP 协议</span>
              <span>v1.0 (Resources/Tools/Prompts)</span>
            </div>
          </div>
        </section>
      </div>

      {/* 添加服务器对话框 */}
      {showAddDialog && (
        <McpServerDialog
          onClose={() => setShowAddDialog(false)}
          onAdd={addServer}
          testConnection={testConnection}
        />
      )}
    </div>
  );
}

function parseOptionalPositiveInt(value: string): number | undefined {
  const trimmed = value.trim();
  if (!trimmed) {
    return undefined;
  }
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return 0;
  }
  return parsed;
}
