import { CheckCircle2, CircleDashed, Download, FileText } from 'lucide-react';
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useDataSources } from '@/features/case/hooks';
import {
  deriveEvidenceHashStatus,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  useImportEventState,
} from '@/features/jobs/import-event-state';
import { useReportHistory, useReportTemplates } from '@/features/reports/hooks';
import { exportHtmlReport, exportCsvReport, exportJsonReport } from '@/lib/api/reports';
import { toast } from 'sonner';
import type { ExportScope } from '@/types/models';

export function Reports() {
  const { data: templates } = useReportTemplates();
  const { data: history } = useReportHistory();
  const { data: dataSources } = useDataSources();
  const importSignals = useImportEventState();
  const [selectedFormat, setSelectedFormat] = useState('html');
  const [exportScope, setExportScope] = useState<ExportScope>({
    fileSystemMetadata: true,
    registry: true,
    fullTimeline: true,
    rawFileExtraction: false,
  });
  const qc = useQueryClient();
  const exportMutation = useMutation({
    mutationFn: () => {
      if (selectedFormat === 'csv') return exportCsvReport(exportScope);
      if (selectedFormat === 'json') return exportJsonReport(exportScope);
      return exportHtmlReport(exportScope);
    },
    onSuccess: (r) => { toast.success('报告生成成功', { description: r }); qc.invalidateQueries({ queryKey: ['reports'] }); },
    onError: (e: Error) => { toast.error('报告生成失败', { description: e.message }); },
  });
  const runningCount = history?.filter((item) => item.status === 'running').length ?? 0;
  const completedCount = history?.filter((item) => item.status === 'completed').length ?? 0;
  const evidenceHashStatus = deriveEvidenceHashStatus(importSignals.partialResults, dataSources ?? []);

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] shrink-0 p-4">
        <div className="flex items-center justify-between gap-4 mb-3">
          <div className="text-[#888] text-[10px] uppercase tracking-wider">选择模板</div>
          <div className="font-mono text-[10px] text-[#666]">运行中 {runningCount} / 已完成 {completedCount}</div>
        </div>
        <div className="flex gap-4 overflow-auto">
          {templates?.map((template, index) => (
            <div
              key={template.id}
              className={`relative p-3 w-64 cursor-pointer ${index === 0 ? 'border border-[#111] bg-white' : 'border border-[#e0e0e0] bg-[#f9f9f9] hover:border-[#aaa]'} transition-colors`}
            >
              {index === 0 ? (
                <div className="absolute right-3 top-3">
                  <CheckCircle2 size={14} className="text-[#111]" />
                </div>
              ) : null}
              <div className="text-[#111] text-[13px] font-medium mb-1">{template.name}</div>
              <div className="text-[#666] text-[11px] leading-relaxed">{template.description}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="flex-1 border-r border-[#e0e0e0] p-6 bg-white flex flex-col gap-6 min-h-0">
          <div>
            <div className="text-[#888] text-[10px] uppercase tracking-wider mb-3">导出范围</div>
            <div className="space-y-2 font-mono text-[11px] text-[#333]">
              <label className="flex items-center gap-2 cursor-pointer">
                <input type="checkbox" checked={exportScope.fileSystemMetadata} onChange={(e) => setExportScope((s) => ({ ...s, fileSystemMetadata: e.target.checked }))} className="accent-[#111]" /> 包含文件系统元数据
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <input type="checkbox" checked={exportScope.registry} onChange={(e) => setExportScope((s) => ({ ...s, registry: e.target.checked }))} className="accent-[#111]" /> 包含注册表项
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <input type="checkbox" checked={exportScope.fullTimeline} onChange={(e) => setExportScope((s) => ({ ...s, fullTimeline: e.target.checked }))} className="accent-[#111]" /> 包含完整时间线
              </label>
              <label className="flex items-center gap-2 cursor-pointer text-[#888]">
                <input type="checkbox" checked={exportScope.rawFileExtraction} onChange={(e) => setExportScope((s) => ({ ...s, rawFileExtraction: e.target.checked }))} className="accent-[#111]" /> 包含原始文件提取（会增加文件大小）
              </label>
            </div>
          </div>

          <div>
            <div className="text-[#888] text-[10px] uppercase tracking-wider mb-3">格式</div>
            <select value={selectedFormat} onChange={(e) => setSelectedFormat(e.target.value)} className="bg-white border border-[#ccc] text-[#111] font-mono text-[11px] p-2 outline-none w-64 focus:border-[#111]">
              <option value="html">HTML</option>
              <option value="csv">CSV</option>
              <option value="json">JSON</option>
            </select>
          </div>

          <div className="border border-[#e0e0e0] bg-[#fafafa] p-3 text-[11px] text-[#555] space-y-1">
            <div className="font-medium text-[#111]">导出摘要</div>
            <div>当前模板将生成案件执行摘要、关键时间线与核心痕迹清单。</div>
            <div className="font-mono text-[#888]">预计产物: 1 份 PDF / 14-18 页</div>
          </div>

          {evidenceHashStatus ? (
            <div className="border border-[#e7d9b4] bg-[#fff9ec] p-3 text-[11px] text-[#6f4d00] space-y-1">
              <div className="font-semibold text-[#111]">Evidence Hash: {getEvidenceHashStatusLabel(evidenceHashStatus)}</div>
              <div>{getEvidenceHashCaveatText(evidenceHashStatus)}</div>
            </div>
          ) : null}

          <div className="mt-4">
            <button onClick={() => exportMutation.mutate()} disabled={exportMutation.isPending} className="bg-[#111] text-white font-semibold text-[11px] px-6 py-2 uppercase tracking-wider hover:bg-[#333] flex items-center gap-2 transition-colors disabled:opacity-50">
              <Download size={14} /> {exportMutation.isPending ? "生成中..." : "生成报告"}
            </button>
          </div>
        </div>

        <div className="w-96 bg-[#fafafa] flex flex-col shrink-0 min-h-0">
          <div className="h-7 border-b border-[#e0e0e0] flex items-center justify-between px-4 text-[10px] font-semibold uppercase text-[#555] tracking-wider shrink-0 bg-[#f5f5f5]">
            <span>最近导出</span>
            <span className="font-mono text-[#888]">{history?.length ?? 0} 条记录</span>
          </div>
          <div className="flex-1 overflow-auto p-4 space-y-4">
            {history?.map((item) => (
              <div key={item.id} className="border border-[#e0e0e0] bg-white p-3">
                <div className="flex justify-between items-center mb-2 gap-4">
                  <div className="text-[#111] text-[12px] font-medium flex items-center gap-1.5 min-w-0 truncate">
                    {item.status === 'running' ? (
                      <CircleDashed size={12} className="animate-spin text-[#888] shrink-0" />
                    ) : (
                      <FileText size={12} className="shrink-0" />
                    )}
                    <span className="truncate">{item.fileName}</span>
                  </div>
                  <div className="text-[#888] font-mono text-[10px] shrink-0">{item.status === 'running' ? '处理中' : '已完成'}</div>
                </div>
                <div className="text-[#666] text-[11px] mb-3">由 {item.createdBy} 生成于 {item.createdAt}</div>
                {item.status === 'running' ? (
                  <div className="space-y-2">
                    <div className="h-1 bg-[#eee] border border-[#e0e0e0] w-full overflow-hidden">
                      <div className="h-full bg-[#111]" style={{ width: `${item.progress ?? 0}%` }}></div>
                    </div>
                    <div className="font-mono text-[10px] text-[#888]">导出队列处理中，正在写入时间线与附件章节。</div>
                  </div>
                ) : (
                  <button className="text-[10px] text-[#333] border border-[#ccc] px-2 py-1 hover:bg-[#f0f0f0] transition-colors w-full uppercase tracking-wider font-medium">下载</button>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
