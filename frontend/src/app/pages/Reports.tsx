import { CheckCircle2, CircleDashed, Download, FileText } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { useDataSources } from '@/features/case/hooks';
import {
  deriveEvidenceHashStatus,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  useImportEventState,
} from '@/features/jobs/import-event-state';
import { useExportReport, useReportHistory, useReportTemplates, type ReportExportFormat } from '@/features/reports/hooks';
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
  const exportMutation = useExportReport();
  const selectedExportFormat = selectedFormat as ReportExportFormat;
  const runExport = () => exportMutation.mutate({ format: selectedExportFormat, scope: exportScope }, {
    onSuccess: (r) => { toast.success('报告生成成功', { description: r }); },
    onError: (e: Error) => { toast.error('报告生成失败', { description: e.message }); },
  });
  const runningCount = history?.filter((item) => item.status === 'running').length ?? 0;
  const completedCount = history?.filter((item) => item.status === 'completed').length ?? 0;
  const evidenceHashStatus = deriveEvidenceHashStatus(importSignals.partialResults, dataSources ?? []);

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-forensics-surface min-w-0">
      <div className="border-b border-forensics-border bg-forensics-panel shrink-0 p-4">
        <div className="flex items-center justify-between gap-4 mb-3">
          <div className="text-forensics-muted-light text-[10px] uppercase tracking-wider">选择模板</div>
          <div className="font-mono text-[10px] text-forensics-muted">运行中 {runningCount} / 已完成 {completedCount}</div>
        </div>
        <div className="flex gap-4 overflow-auto">
          {templates?.map((template, index) => (
            <div
              key={template.id}
              className={`relative p-3 w-64 cursor-pointer ${index === 0 ? 'border border-forensics-text bg-forensics-surface' : 'border border-forensics-border bg-forensics-surface hover:border-forensics-border-strong'} transition-colors`}
            >
              {index === 0 ? (
                <div className="absolute right-3 top-3">
                  <CheckCircle2 size={14} className="text-forensics-text" />
                </div>
              ) : null}
              <div className="text-forensics-text text-[13px] font-light mb-1">{template.name}</div>
              <div className="text-forensics-muted text-[11px] leading-relaxed line-clamp-3">{template.description}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="flex-1 border-r border-forensics-border p-6 bg-forensics-surface flex flex-col gap-6 min-h-0 min-w-0">
          <div>
            <div className="text-forensics-muted-light text-[10px] uppercase tracking-wider mb-3">导出范围</div>
            <div className="space-y-2 font-mono text-[11px] text-forensics-text-secondary">
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox variant="forensics" checked={exportScope.fileSystemMetadata} onCheckedChange={(checked) => setExportScope((s) => ({ ...s, fileSystemMetadata: checked === true }))} /> 包含文件系统元数据
              </label>
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox variant="forensics" checked={exportScope.registry} onCheckedChange={(checked) => setExportScope((s) => ({ ...s, registry: checked === true }))} /> 包含注册表项
              </label>
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox variant="forensics" checked={exportScope.fullTimeline} onCheckedChange={(checked) => setExportScope((s) => ({ ...s, fullTimeline: checked === true }))} /> 包含完整时间线
              </label>
              <label className="flex cursor-pointer items-center gap-2 text-forensics-muted-light">
                <Checkbox variant="forensics" checked={exportScope.rawFileExtraction} onCheckedChange={(checked) => setExportScope((s) => ({ ...s, rawFileExtraction: checked === true }))} /> 包含原始文件提取（会增加文件大小）
              </label>
            </div>
          </div>

          <div>
            <div className="text-forensics-muted-light text-[10px] uppercase tracking-wider mb-3">格式</div>
            <Select value={selectedFormat} onValueChange={setSelectedFormat}>
              <SelectTrigger variant="mono" size="sm" className="w-64">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="html">HTML</SelectItem>
                <SelectItem value="csv">CSV</SelectItem>
                <SelectItem value="json">JSON</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="border border-forensics-border bg-forensics-panel p-3 text-[11px] text-forensics-text-tertiary space-y-1">
            <div className="font-light text-forensics-text">导出摘要</div>
            <div>当前模板将生成案件执行摘要、关键时间线与核心痕迹清单。</div>
            <div className="font-mono text-forensics-muted-light">
              预计产物: {exportScope.rawFileExtraction ? '报告 + 原始文件批量导出清单 + SHA256SUMS' : '报告主体文件'}
            </div>
          </div>

          {evidenceHashStatus ? (
            <div className="border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text space-y-1">
              <div className="font-light text-forensics-text">Evidence Hash: {getEvidenceHashStatusLabel(evidenceHashStatus)}</div>
              <div>{getEvidenceHashCaveatText(evidenceHashStatus)}</div>
            </div>
          ) : null}

          <div className="mt-4">
            <Button
              type="button"
              variant="forensicsPrimary"
              size="sm"
              onClick={runExport}
              disabled={exportMutation.isPending}
              className="px-6 font-light uppercase tracking-wider"
            >
              <Download size={14} /> {exportMutation.isPending ? "生成中..." : "生成报告"}
            </Button>
          </div>
        </div>

        <div className="w-96 max-w-[40%] bg-forensics-panel flex flex-col shrink-0 min-h-0">
          <div className="h-7 border-b border-forensics-border flex items-center justify-between px-4 text-[10px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0 bg-forensics-panel-strong">
            <span>最近导出</span>
            <span className="font-mono text-forensics-muted-light">{history?.length ?? 0} 条记录</span>
          </div>
          <div className="flex-1 overflow-auto p-4 space-y-4">
            {history?.map((item) => (
              <div key={item.id} className="border border-forensics-border bg-forensics-surface p-3">
                <div className="flex justify-between items-center mb-2 gap-4">
                  <div className="text-forensics-text text-[12px] font-light flex items-center gap-1.5 min-w-0 truncate">
                    {item.status === 'running' ? (
                      <CircleDashed size={12} className="opacity-70 text-forensics-muted-light shrink-0" />
                    ) : (
                      <FileText size={12} className="shrink-0" />
                    )}
                    <span className="truncate">{item.fileName}</span>
                  </div>
                  <div className="text-forensics-muted-light font-mono text-[10px] shrink-0">{item.status === 'running' ? '处理中' : '已完成'}</div>
                </div>
                <div className="text-forensics-muted text-[11px] mb-3">由 {item.createdBy} 生成于 {item.createdAt}</div>
                {item.status === 'running' ? (
                  <div className="space-y-2">
                    <div className="h-1 bg-forensics-panel-strong border border-forensics-border w-full overflow-hidden">
                      <div className="h-full bg-forensics-text" style={{ width: `${item.progress ?? 0}%` }}></div>
                    </div>
                    <div className="font-mono text-[10px] text-forensics-muted-light">导出队列处理中，正在写入时间线与附件章节。</div>
                  </div>
                ) : (
                  <Button
                    type="button"
                    variant="forensicsOutline"
                    size="compact"
                    className="w-full text-[10px] font-light uppercase tracking-wider"
                  >
                    下载
                  </Button>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
