import { CheckCircle2, CircleDashed, Download, FileText } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import type { ReportsWorkspaceModel } from '@/features/reports/use-reports-workspace-model';

interface ReportsWorkspaceProps {
  model: ReportsWorkspaceModel;
}

/** Pure report presentation surface. Export orchestration and feedback stay in the workspace model. */
export function ReportsWorkspace({ model }: ReportsWorkspaceProps) {
  return (
    <div className="flex h-full w-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-4">
        <div className="mb-3 flex items-center justify-between gap-4"><div className="text-[10px] uppercase tracking-wider text-forensics-muted-light">选择模板</div><div className="font-mono text-[10px] text-forensics-muted">运行中 {model.runningCount} / 已完成 {model.completedCount}</div></div>
        <div className="flex gap-4 overflow-x-auto overflow-y-hidden">
          {model.reportTemplates?.map((template, index) => <div key={template.id} className={`relative w-64 cursor-pointer p-3 transition-colors ${index === 0 ? 'border border-forensics-text bg-forensics-surface' : 'border border-forensics-border bg-forensics-surface hover:border-forensics-border-strong'}`}>
            {index === 0 ? <div className="absolute right-3 top-3"><CheckCircle2 size={14} className="text-forensics-text" /></div> : null}
            <div className="mb-1 text-[13px] font-light text-forensics-text">{template.name}</div><div className="line-clamp-3 text-[11px] leading-relaxed text-forensics-muted">{template.description}</div>
          </div>)}
        </div>
      </div>
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-6 border-r border-forensics-border bg-forensics-surface p-6">
          <div><div className="mb-3 text-[10px] uppercase tracking-wider text-forensics-muted-light">导出范围</div><div className="space-y-2 font-mono text-[11px] text-forensics-text-secondary">
            <label className="flex cursor-pointer items-center gap-2"><Checkbox variant="forensics" checked={model.exportScope.fileSystemMetadata} onCheckedChange={(checked) => model.setScopeOption('fileSystemMetadata', checked === true)} /> 包含文件系统元数据</label>
            <label className="flex cursor-pointer items-center gap-2"><Checkbox variant="forensics" checked={model.exportScope.registry} onCheckedChange={(checked) => model.setScopeOption('registry', checked === true)} /> 包含注册表项</label>
            <label className="flex cursor-pointer items-center gap-2"><Checkbox variant="forensics" checked={model.exportScope.fullTimeline} onCheckedChange={(checked) => model.setScopeOption('fullTimeline', checked === true)} /> 包含完整时间线</label>
            <label className="flex cursor-pointer items-center gap-2 text-forensics-muted-light"><Checkbox variant="forensics" checked={model.exportScope.rawFileExtraction} onCheckedChange={(checked) => model.setScopeOption('rawFileExtraction', checked === true)} /> 包含原始文件提取（会增加文件大小）</label>
          </div></div>
          <div><div className="mb-3 text-[10px] uppercase tracking-wider text-forensics-muted-light">格式</div><Select value={model.selectedFormat} onValueChange={model.selectFormat}><SelectTrigger variant="mono" size="sm" className="w-64"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="html">HTML</SelectItem><SelectItem value="csv">CSV</SelectItem><SelectItem value="json">JSON</SelectItem></SelectContent></Select></div>
          <div className="space-y-1 border border-forensics-border bg-forensics-panel p-3 text-[11px] text-forensics-text-tertiary"><div className="font-light text-forensics-text">导出摘要</div><div>当前模板将生成案件执行摘要、关键时间线与核心痕迹清单。</div><div className="font-mono text-forensics-muted-light">预计产物: {model.exportScope.rawFileExtraction ? '报告 + 原始文件批量导出清单 + SHA256SUMS' : '报告主体文件'}</div></div>
          {model.evidenceHashLabel ? <div className="space-y-1 border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text"><div className="font-light text-forensics-text">Evidence Hash: {model.evidenceHashLabel}</div><div>{model.evidenceHashCaveat}</div></div> : null}
          <div className="mt-4"><Button type="button" variant="forensicsPrimary" size="sm" onClick={model.runExport} disabled={model.exportPending} className="px-6 font-light uppercase tracking-wider"><Download size={14} /> {model.exportPending ? '生成中...' : '生成报告'}</Button></div>
        </div>
        <div className="flex min-h-0 w-96 max-w-[40%] shrink-0 flex-col bg-forensics-panel"><div className="flex h-7 shrink-0 items-center justify-between border-b border-forensics-border bg-forensics-panel-strong px-4 text-[10px] font-light uppercase tracking-wider text-forensics-text-tertiary"><span>最近导出</span><span className="font-mono text-forensics-muted-light">{model.history?.length ?? 0} 条记录</span></div>
          <ScrollArea className="min-h-0 flex-1" viewportClassName="space-y-4 p-4">{model.history?.map((item) => <div key={item.id} className="border border-forensics-border bg-forensics-surface p-3"><div className="mb-2 flex items-center justify-between gap-4"><div className="flex min-w-0 items-center gap-1.5 truncate text-[12px] font-light text-forensics-text">{item.status === 'running' ? <CircleDashed size={12} className="shrink-0 text-forensics-muted-light opacity-70" /> : <FileText size={12} className="shrink-0" />}<span className="truncate">{item.fileName}</span></div><div className="shrink-0 font-mono text-[10px] text-forensics-muted-light">{item.status === 'running' ? '处理中' : '已完成'}</div></div><div className="mb-3 text-[11px] text-forensics-muted">由 {item.createdBy} 生成于 {item.createdAt}</div>{item.status === 'running' ? <div className="space-y-2"><div className="h-1 w-full overflow-hidden border border-forensics-border bg-forensics-panel-strong"><div className="h-full bg-forensics-text" style={{ width: `${item.progress ?? 0}%` }} /></div><div className="font-mono text-[10px] text-forensics-muted-light">导出队列处理中，正在写入时间线与附件章节。</div></div> : <Button type="button" variant="forensicsOutline" size="compact" className="w-full text-[10px] font-light uppercase tracking-wider">下载</Button>}</div>)}</ScrollArea>
        </div>
      </div>
    </div>
  );
}
