import { FolderOpen, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import type { JobSnapshot, RecentCase } from '@/types/models';
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
  onRequestDeleteCase: (recentCase: RecentCase) => void;
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
  onRequestDeleteCase,
}: CaseWelcomeFormsProps) {
  const { t } = useTranslation();
  return (
    <ScrollArea className="min-h-0 flex-1 bg-forensics-surface" viewportClassName="flex min-h-full flex-col">
      <div className="border-b border-forensics-border bg-forensics-panel p-8">
        <div className="font-display text-3xl text-forensics-text tracking-tight mb-3">{BRAND_DISPLAY_NAME}</div>
        <div className="max-w-3xl text-[14px] text-forensics-muted leading-7">
          {t('caseHome.welcome.description')}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-6 p-8">
        <div className="border border-forensics-border bg-forensics-surface p-5">
          <div className="text-[13px] font-light text-forensics-text-secondary mb-3">{t('caseHome.welcome.create.title')}</div>
          <div className="space-y-2 mb-3">
            <Input
              type="text"
              value={caseRoot}
              onChange={(e) => setCaseRoot(e.target.value)}
              placeholder={t('caseHome.welcome.create.rootPlaceholder')}
              variant="path"
              inputSize="compact"
            />
            <Input
              type="text"
              value={caseName}
              onChange={(e) => setCaseName(e.target.value)}
              placeholder={t('caseHome.welcome.create.namePlaceholder')}
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
            {createPending ? t('caseHome.welcome.create.pending') : t('caseHome.welcome.create.action')}
          </Button>
          {createError ? (
            <div className="mt-2 text-[11px] text-forensics-error-text">{createError}</div>
          ) : null}
        </div>

        <div className="border border-forensics-border bg-forensics-surface p-5">
          <div className="text-[13px] font-light text-forensics-text-secondary mb-3">{t('caseHome.welcome.open.title')}</div>
          <div className="space-y-2 mb-3">
            <Input
              type="text"
              value={openCasePath}
              onChange={(e) => setOpenCasePath(e.target.value)}
              placeholder={t('caseHome.welcome.open.pathPlaceholder')}
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
            {openPending ? t('caseHome.welcome.open.pending') : t('caseHome.welcome.open.action')}
          </Button>
          {openError ? (
            <div className="mt-2 text-[11px] text-forensics-error-text">{openError}</div>
          ) : null}
        </div>
      </div>

      <div className="px-8 pb-8">
        <div className="border border-forensics-border bg-forensics-surface">
          <div className="border-b border-forensics-border bg-forensics-panel px-5 py-3 flex items-center justify-between">
            <div className="text-[13px] font-light text-forensics-text-secondary">{t('caseHome.welcome.recent.title')}</div>
            <div className="text-[10px] font-mono text-forensics-muted-light">{t('caseHome.count', { count: recentCases.length })}</div>
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
                    <div className="text-[13px] text-forensics-text font-light truncate">{item.name}</div>
                    <div className="text-[11px] text-forensics-muted font-mono truncate mt-1">{item.caseRoot}</div>
                  </div>
                  <div className="text-[10px] text-forensics-muted-light font-mono shrink-0 mr-3">{item.openedAt}</div>
                  <Button
                    type="button"
                    variant="forensicsDangerGhost"
                    size="iconSm"
                    onClick={(e) => {
                      e.stopPropagation();
                      onRequestDeleteCase(item);
                    }}
                    className="shrink-0"
                    title={t('caseHome.actions.deleteCase')}
                    aria-label={t('caseHome.actions.deleteCase')}
                  >
                    <Trash2 size={12} />
                  </Button>
                </div>
              ))}
            </div>
          ) : (
            <div className="px-5 py-6 text-[12px] text-forensics-muted">{t('caseHome.welcome.recent.empty')}</div>
          )}
        </div>
      </div>
    </ScrollArea>
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
  onBrowseFile: () => Promise<string | undefined>;
  onBrowseDirectory: () => Promise<string | undefined>;
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
  onBrowseFile,
  onBrowseDirectory,
}: ImportSectionProps) {
  const { t } = useTranslation();
  return (
    <div className="border-b border-forensics-border bg-forensics-panel p-4 shrink-0">
      <div className="flex items-center gap-3">
        <Input
          type="text"
          value={importPath}
          onChange={(e) => setImportPath(e.target.value)}
          placeholder={t('caseHome.import.pathPlaceholder')}
          variant="path"
          inputSize="compact"
          className="flex-1"
        />
        <Button
          type="button"
          variant="forensicsOutline"
          size="xs"
          onClick={async () => {
            const path = await onBrowseFile();
            if (path) {
              setImportPath(path);
            }
          }}
        >
          <FolderOpen size={12} /> {t('caseHome.import.file')}
        </Button>
        <Button
          type="button"
          variant="forensicsOutline"
          size="xs"
          onClick={async () => {
            const path = await onBrowseDirectory();
            if (path) {
              setImportPath(path);
            }
          }}
        >
          <FolderOpen size={12} /> {t('caseHome.import.directory')}
        </Button>
        <Button
          type="button"
          variant="forensicsPrimary"
          size="xs"
          onClick={onImport}
          disabled={importPending || Boolean(importJob)}
        >
          {importPending ? t('caseHome.import.submitting') : importJob ? t('caseHome.import.running') : t('caseHome.import.action')}
        </Button>
        <Button
          type="button"
          variant="forensicsGhost"
          size="xs"
          onClick={() => {
            onClose();
          }}
        >
          {t('caseHome.actions.cancel')}
        </Button>
      </div>
      {importPending ? (
        <div className="mt-2 flex items-center gap-2 text-[11px] text-forensics-muted">
          <div className="w-3 h-3 border-2 border-forensics-muted border-t-transparent rounded-none opacity-70" />
          {t('caseHome.import.submitHint')}
        </div>
      ) : null}
      {importJob ? (
        <div className="mt-2 text-[11px] text-forensics-text-tertiary font-mono bg-forensics-surface border border-forensics-350 p-2">
          <div>{t('caseHome.import.progress', { name: importJob.name, progress: importJob.progress, detail: importJob.detail })}</div>
          <Button
            type="button"
            variant="forensicsLink"
            size="inline"
            onClick={onCancelImport}
            disabled={cancelImportPending}
            className="mt-1 text-[10px] text-forensics-error-text hover:text-forensics-error-text"
          >
            {cancelImportPending ? t('caseHome.import.canceling') : t('caseHome.import.cancel')}
          </Button>
        </div>
      ) : null}
      {importSuccess ? (
        <div className="mt-2 text-[11px] text-forensics-success-text font-mono bg-forensics-success-bg border border-forensics-success-border p-2">
          {importSuccess}
        </div>
      ) : null}
      {importError ? (
        <div className="mt-2 text-[11px] text-forensics-error-text font-mono bg-forensics-error-bg border border-forensics-error-border p-2">
          {t('caseHome.import.failed', { error: importError })}
        </div>
      ) : null}
      {failedImportJob ? (
        <div className="mt-2 text-[11px] text-forensics-error-text font-mono bg-forensics-error-bg border border-forensics-error-border p-2">
          {t('caseHome.import.backgroundFailed', { detail: failedImportJob.detail || failedImportJob.name })}
        </div>
      ) : null}
    </div>
  );
}
