/**
 * DataAnalysis - 数据源分析页面
 *
 * 功能：
 * - 系统信息展示
 * - 文件分类统计
 * - 分析报告生成
 */

import { useState, useEffect } from 'react';
import {
  Monitor,
  HardDrive,
  Network,
  Clock,
  FileText,
  Image,
  Archive,
  Database,
  Shield,
  Download,
  RefreshCw,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

interface SystemInfo {
  computer_name?: string;
  os_version?: string;
  build_number?: string;
  install_date?: string;
  registered_owner?: string;
  timezone?: string;
  network_adapters: Array<{
    name: string;
    mac_address?: string;
    ip_addresses: string[];
  }>;
  boot_history: Array<{
    timestamp: string;
    boot_type: string;
    source: string;
  }>;
}

interface FileClassification {
  category: string;
  files: Array<{
    path: string;
    name: string;
    size: number;
    file_type: string;
    magic_description: string;
  }>;
  total_size: number;
}

const CATEGORY_ICONS: Record<string, typeof Monitor> = {
  Executables: Shield,
  Documents: FileText,
  Images: Image,
  Archives: Archive,
  Databases: Database,
  System: HardDrive,
  Forensics: Monitor,
  Logs: FileText,
  Other: FileText,
};

const CATEGORY_COLORS: Record<string, string> = {
  Executables: '#e74c3c',
  Documents: '#3498db',
  Images: '#2ecc71',
  Archives: '#f39c12',
  Databases: '#9b59b6',
  System: '#7f8c8d',
  Forensics: '#1abc9c',
  Logs: '#34495e',
  Other: '#95a5a6',
};

export function DataAnalysis() {
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [classifications, setClassifications] = useState<FileClassification[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'system' | 'files' | 'report'>('system');

  // Load data
  const loadData = async () => {
    setLoading(true);
    setError(null);
    try {
      const [info, classes] = await Promise.all([
        invoke<SystemInfo>('get_system_info'),
        invoke<FileClassification[]>('classify_files', { sampleSize: 1000 }),
      ]);
      setSystemInfo(info);
      setClassifications(classes);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  // Format file size
  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  };

  // Calculate total stats
  const totalFiles = classifications.reduce((sum, c) => sum + c.files.length, 0);
  const totalSize = classifications.reduce((sum, c) => sum + c.total_size, 0);

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      {/* Header */}
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="flex items-center justify-between">
          <div>
            <div className="font-serif text-xl text-[#111] tracking-tight">
              数据源分析
            </div>
            <div className="text-[#666] text-[11px] font-mono mt-1">
              系统信息 · 文件分类 · 证据分析
            </div>
          </div>
          <button
            onClick={loadData}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 text-[12px] bg-white border border-[#ddd] rounded hover:bg-[#f5f5f5] disabled:opacity-50"
          >
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            刷新
          </button>
        </div>
      </div>

      {/* Tab navigation */}
      <div className="flex border-b border-[#e0e0e0] bg-[#fafafa]">
        {[
          { key: 'system', label: '系统信息', icon: Monitor },
          { key: 'files', label: '文件分类', icon: FileText },
          { key: 'report', label: '分析报告', icon: Download },
        ].map(({ key, label, icon: Icon }) => (
          <button
            key={key}
            onClick={() => setActiveTab(key as any)}
            className={`flex items-center gap-2 px-6 py-3 text-[12px] border-b-2 transition-colors ${
              activeTab === key
                ? 'border-[#111] text-[#111] font-medium'
                : 'border-transparent text-[#666] hover:text-[#111]'
            }`}
          >
            <Icon size={14} />
            {label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 p-6 overflow-auto">
        {error && (
          <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded text-[12px] text-red-700">
            {error}
          </div>
        )}

        {loading ? (
          <div className="flex items-center justify-center h-64 text-[#999]">
            <RefreshCw size={24} className="animate-spin mr-2" />
            正在分析数据源...
          </div>
        ) : (
          <>
            {/* System Info Tab */}
            {activeTab === 'system' && systemInfo && (
              <div className="space-y-6">
                {/* Basic Info */}
                <section>
                  <h3 className="text-[14px] font-semibold text-[#111] mb-3 flex items-center gap-2">
                    <Monitor size={16} />
                    系统信息
                  </h3>
                  <div className="grid grid-cols-2 gap-4">
                    <InfoCard label="计算机名" value={systemInfo.computer_name} />
                    <InfoCard label="操作系统" value={systemInfo.os_version} />
                    <InfoCard label="Build 号" value={systemInfo.build_number} />
                    <InfoCard label="注册用户" value={systemInfo.registered_owner} />
                    <InfoCard label="时区" value={systemInfo.timezone} />
                    <InfoCard label="安装日期" value={systemInfo.install_date} />
                  </div>
                </section>

                {/* Network Adapters */}
                {systemInfo.network_adapters.length > 0 && (
                  <section>
                    <h3 className="text-[14px] font-semibold text-[#111] mb-3 flex items-center gap-2">
                      <Network size={16} />
                      网络适配器
                    </h3>
                    <div className="space-y-2">
                      {systemInfo.network_adapters.map((adapter, i) => (
                        <div
                          key={i}
                          className="p-3 bg-[#f8f8f8] border border-[#e0e0e0] rounded"
                        >
                          <div className="text-[12px] font-medium">{adapter.name}</div>
                          {adapter.mac_address && (
                            <div className="text-[11px] text-[#666] font-mono">
                              MAC: {adapter.mac_address}
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  </section>
                )}

                {/* Boot History */}
                {systemInfo.boot_history.length > 0 && (
                  <section>
                    <h3 className="text-[14px] font-semibold text-[#111] mb-3 flex items-center gap-2">
                      <Clock size={16} />
                      开关机历史
                    </h3>
                    <div className="space-y-1">
                      {systemInfo.boot_history.map((boot, i) => (
                        <div
                          key={i}
                          className="flex items-center gap-3 p-2 text-[12px]"
                        >
                          <span className="text-[#666] font-mono">{boot.timestamp}</span>
                          <span className="px-2 py-0.5 bg-[#f0f0f0] rounded text-[10px]">
                            {boot.boot_type}
                          </span>
                          <span className="text-[#999]">{boot.source}</span>
                        </div>
                      ))}
                    </div>
                  </section>
                )}
              </div>
            )}

            {/* File Classification Tab */}
            {activeTab === 'files' && (
              <div className="space-y-6">
                {/* Summary */}
                <div className="grid grid-cols-3 gap-4">
                  <StatCard label="总文件数" value={totalFiles.toString()} />
                  <StatCard label="总大小" value={formatSize(totalSize)} />
                  <StatCard label="分类数" value={classifications.length.toString()} />
                </div>

                {/* Categories */}
                <div className="space-y-4">
                  {classifications.map((cat) => {
                    const Icon = CATEGORY_ICONS[cat.category] || FileText;
                    const color = CATEGORY_COLORS[cat.category] || '#95a5a6';

                    return (
                      <section key={cat.category}>
                        <div className="flex items-center gap-2 mb-2">
                          <Icon size={16} style={{ color }} />
                          <h3 className="text-[14px] font-semibold text-[#111]">
                            {cat.category}
                          </h3>
                          <span className="text-[11px] text-[#999]">
                            {cat.files.length} 个文件 · {formatSize(cat.total_size)}
                          </span>
                        </div>
                        <div className="bg-[#f8f8f8] border border-[#e0e0e0] rounded overflow-hidden">
                          <table className="w-full text-[11px]">
                            <thead>
                              <tr className="bg-[#f0f0f0]">
                                <th className="px-3 py-2 text-left font-medium">文件名</th>
                                <th className="px-3 py-2 text-left font-medium">类型</th>
                                <th className="px-3 py-2 text-right font-medium">大小</th>
                              </tr>
                            </thead>
                            <tbody>
                              {cat.files.slice(0, 20).map((file, i) => (
                                <tr key={i} className="border-t border-[#e0e0e0]">
                                  <td className="px-3 py-1.5 font-mono truncate max-w-[300px]">
                                    {file.name}
                                  </td>
                                  <td className="px-3 py-1.5 text-[#666]">
                                    {file.magic_description}
                                  </td>
                                  <td className="px-3 py-1.5 text-right text-[#666]">
                                    {formatSize(file.size)}
                                  </td>
                                </tr>
                              ))}
                              {cat.files.length > 20 && (
                                <tr className="border-t border-[#e0e0e0]">
                                  <td
                                    colSpan={3}
                                    className="px-3 py-1.5 text-center text-[#999]"
                                  >
                                    还有 {cat.files.length - 20} 个文件...
                                  </td>
                                </tr>
                              )}
                            </tbody>
                          </table>
                        </div>
                      </section>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Report Tab */}
            {activeTab === 'report' && (
              <div className="flex flex-col items-center justify-center h-64 gap-4">
                <FileText size={48} className="text-[#ccc]" />
                <div className="text-[14px] text-[#666]">生成分析报告</div>
                <button
                  onClick={async () => {
                    try {
                      const summary = await invoke<string>('generate_analysis_summary');
                      const blob = new Blob([summary], { type: 'text/markdown' });
                      const url = URL.createObjectURL(blob);
                      const a = document.createElement('a');
                      a.href = url;
                      a.download = 'analysis-report.md';
                      a.click();
                      URL.revokeObjectURL(url);
                    } catch (err) {
                      setError(String(err));
                    }
                  }}
                  className="px-6 py-2 bg-[#111] text-white rounded hover:bg-[#333] text-[12px]"
                >
                  下载 Markdown 报告
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

// Helper components
function InfoCard({ label, value }: { label: string; value?: string }) {
  return (
    <div className="p-3 bg-[#f8f8f8] border border-[#e0e0e0] rounded">
      <div className="text-[10px] text-[#999] uppercase tracking-wider mb-1">
        {label}
      </div>
      <div className="text-[13px] font-mono text-[#111]">
        {value || '-'}
      </div>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="p-4 bg-[#f8f8f8] border border-[#e0e0e0] rounded text-center">
      <div className="text-[24px] font-bold text-[#111]">{value}</div>
      <div className="text-[11px] text-[#666] mt-1">{label}</div>
    </div>
  );
}
