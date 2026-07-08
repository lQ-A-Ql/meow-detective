import { FolderOpen, Trash2 } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import type { JobSnapshot, RecentCase } from '@/types/models';
import { openDialog, singleDialogPath } from '@/lib/platform/dialog';
import { BRAND_DISPLAY_NAME } from '@/lib/branding';

// ── Welcome screen: create + open case forms ──

export interface CaseWelcomeFormsProps {
  caseRoot: string;
  setCaseRoot: (v: string) => void;
  caseName: string;
  setCaseName: (v: string) => void;
  onCreateCase: () => void;
  createPending: boolean;
  createError: string | null;
  openCasePath: string;
  setOpenCasePath: (v: string) => void;
  onOpenCase: (path: string) => void;
  openPending: boolean;
  openError: string | null;
  recentCases: RecentCase[];
  onDeleteCase: (caseRoot: string) => void;
}

export function CaseWelcomeForms({
  caseRoot,
  setCaseRoot,
  caseName,
  setCaseName,
  onCreateCase,
  createPending,
  createError,
  openCasePath,
  setOpenCasePath,
  onOpenCase,
  openPending,
  openError,
  recentCases,
  onDeleteCase,
}: CaseWelcomeFormsProps) {
  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      <div className="border-b border-forensics-border bg-forensics-panel p-8">
        <div className="font-display text-3xl text-forensics-text tracking-tight mb-3">{BRAND_DISPLAY_NAME}</div>
        <div className="max-w-3xl text-[14px] text-forensics-muted leading-7">
          当前没有活动案件。先创建或打开案件目录，接着导入逻辑目录、RAW/DD/IMG 或 E01 镜像，即可进入真实文件浏览链路。
        </div>
      </div>

      <div className="grid grid-cols-2 gap-6 p-8">
        <div className="border border-forensics-border bg-white p-5">
          <div className="text-[13px] font-semibold text-forensics-text-secondary mb-3">新建案件</div>
          <div className="space-y-2 mb-3">
            <Input
              type="text"
              value={caseRoot}
              onChange={(e) => setCaseRoot(e.target.value)}
              placeholder="案件父目录"
              variant="path"
              inputSize="compact"
            />
            <Input
              type="text"
              value={caseName}
              onChange={(e) => setCaseName(e.target.value)}
              placeholder="案件名称"
              variant="forensics"
              inputSize="compact"
            />
          </div>
          <Button
            type="button"
            variant="forensicsPrimary"
            size="xs"
            onClick={onCreateCase}
            disabled={createPending || !caseRoot || !caseName}
          >
            {createPending ? '创建中...' : '创建案件'}
          </Button>
          {createError ? (
            <div className="mt-2 text-[11px] text-red-600">{createError}</div>
          ) : null}
        </div>

        <div className="border border-forensics-border bg-white p-5">
          <div className="text-[13px] font-semibold text-forensics-text-secondary mb-3">打开已有案件</div>
          <div className="space-y-2 mb-3">
            <Input
              type="text"
              value={openCasePath}
              onChange={(e) => setOpenCasePath(e.target.value)}
              placeholder="案件路径"
              variant="path"
              inputSize="compact"
            />
          </div>
          <Button
            type="button"
            variant="forensicsPrimary"
            size="xs"
            onClick={() => onOpenCase(openCasePath)}
            disabled={openPending || !openCasePath}
          >
            {openPending ? '打开中...' : '打开案件'}
          </Button>
          {openError ? (
            <div className="mt-2 text-[11px] text-red-600">{openError}</div>
          ) : null}
        </div>
      </div>

      <div className="px-8 pb-8">
        <div className="border border-forensics-border bg-white">
          <div className="border-b border-forensics-border bg-forensics-panel px-5 py-3 flex items-center justify-between">
            <div className="text-[13px] font-semibold text-forensics-text-secondary">最近打开案件</div>
            <div className="text-[10px] font-mono text-forensics-muted-light">{recentCases.length} 项</div>
          </div>
          {recentCases.length ? (
            <div className="divide-y divide-forensics-border-light">
              {recentCases.map((item) => (
                <div
                  key={`${item.caseRoot}-${item.openedAt}`}
                  className="flex items-center px-5 py-3 text-left hover:bg-forensics-panel-strong cursor-pointer"
                  onClick={() => onOpenCase(item.caseRoot)}
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] text-forensics-text font-medium truncate">{item.name}</div>
                    <div className="text-[11px] text-forensics-muted font-mono truncate mt-1">{item.caseRoot}</div>
                  </div>
                  <div className="text-[10px] text-forensics-muted-light font-mono shrink-0 mr-3">{item.openedAt}</div>
                  <Button
                    type="button"
                    variant="forensicsDangerGhost"
                    size="iconSm"
                    onClick={(e) => {
                      e.stopPropagation();
                      if (window.confirm(`确定删除案件 "${item.name}"？\n\n该操作将删除案件目录及其所有数据，且不可撤销。`)) {
                        onDeleteCase(item.caseRoot);
                      }
                    }}
                    className="shrink-0"
                    title="删除案件"
                  >
                    <Trash2 size={12} />
                  </Button>
                </div>
              ))}
            </div>
          ) : (
            <div className="px-5 py-6 text-[12px] text-forensics-muted">这里会保留最近打开过的案件，便于重新进入分析现场。</div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Import data source section ──

export interface ImportSectionProps {
  importPath: string;
  setImportPath: (v: string) => void;
  onImport: () => void;
  importPending: boolean;
  importSuccess: string | null;
  importError: string | null;
  importJob: JobSnapshot | undefined;
  cancelImportPending: boolean;
  onCancelImport: () => void;
  failedImportJob: JobSnapshot | undefined;
  onClose: () => void;
}

export function ImportSection({
  importPath,
  setImportPath,
  onImport,
  importPending,
  importSuccess,
  importError,
  importJob,
  cancelImportPending,
  onCancelImport,
  failedImportJob,
  onClose,
}: ImportSectionProps) {
  return (
    <div className="border-b border-forensics-border bg-forensics-panel p-4 shrink-0">
      <div className="flex items-center gap-3">
        <Input
          type="text"
          value={importPath}
          onChange={(e) => setImportPath(e.target.value)}
          placeholder="镜像路径或逻辑目录路径"
          variant="path"
          inputSize="compact"
          className="flex-1"
        />
        <Button
          type="button"
          variant="forensicsOutline"
          size="xs"
          onClick={async () => {
            try {
              const selected = await openDialog({
                directory: false,
                multiple: false,
                filters: [{ name: 'Data Sources', extensions: ['e01', 'E01', 'dd', 'raw', 'img'] }],
              });
              const path = singleDialogPath(selected);
              if (path) {
                setImportPath(path);
              }
            } catch {
              // Tauri dialog may be unavailable in non-tauri mode.
            }
          }}
        >
          <FolderOpen size={12} /> 文件
        </Button>
        <Button
          type="button"
          variant="forensicsOutline"
          size="xs"
          onClick={async () => {
            try {
              const selected = await openDialog({ directory: true, multiple: false });
              const path = singleDialogPath(selected);
              if (path) {
                setImportPath(path);
              }
            } catch {
              // Tauri dialog may be unavailable in non-tauri mode.
            }
          }}
        >
          <FolderOpen size={12} /> 目录
        </Button>
        <Button
          type="button"
          variant="forensicsPrimary"
          size="xs"
          onClick={onImport}
          disabled={importPending || Boolean(importJob)}
        >
          {importPending ? '提交中...' : importJob ? '后台导入中...' : '导入'}
        </Button>
        <Button
          type="button"
          variant="forensicsGhost"
          size="xs"
          onClick={() => {
            onClose();
          }}
        >
          取消
        </Button>
      </div>
      {importPending ? (
        <div className="mt-2 flex items-center gap-2 text-[11px] text-forensics-muted">
          <div className="w-3 h-3 border-2 border-forensics-muted border-t-transparent rounded-full animate-spin" />
          正在提交导入任务，后台进度会在任务列表中持续更新。
        </div>
      ) : null}
      {importJob ? (
        <div className="mt-2 text-[11px] text-forensics-text-tertiary font-mono bg-white border border-forensics-350 p-2">
          <div>后台导入进行中: {importJob.name} · {importJob.progress}% · {importJob.detail}</div>
          <Button
            type="button"
            variant="forensicsLink"
            size="inline"
            onClick={onCancelImport}
            disabled={cancelImportPending}
            className="mt-1 text-[10px] text-red-600 hover:text-red-800"
          >
            {cancelImportPending ? '取消中...' : '取消导入'}
          </Button>
        </div>
      ) : null}
      {importSuccess ? (
        <div className="mt-2 text-[11px] text-green-700 font-mono bg-green-50 border border-green-200 p-2">
          {importSuccess}
        </div>
      ) : null}
      {importError ? (
        <div className="mt-2 text-[11px] text-red-600 font-mono bg-red-50 border border-red-200 p-2">
          导入失败: {importError}
        </div>
      ) : null}
      {failedImportJob ? (
        <div className="mt-2 text-[11px] text-red-700 font-mono bg-red-50 border border-red-200 p-2">
          后台导入失败: {failedImportJob.detail || failedImportJob.name}
        </div>
      ) : null}
    </div>
  );
}
