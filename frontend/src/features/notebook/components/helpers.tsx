import type {
  NotebookEntryStatus,
  NotebookEntryType,
} from '@/types/models';

export const ENTRY_TYPE_CONFIG: Record<NotebookEntryType, { label: string; order: number }> = {
  observation: { label: '观察', order: 0 },
  hypothesis: { label: '假设', order: 1 },
  finding: { label: '发现', order: 2 },
  actionItem: { label: '行动项', order: 3 },
  conclusion: { label: '结论', order: 4 },
};

export const ENTRY_TYPE_BADGE: Record<NotebookEntryType, string> = {
  observation: 'bg-forensics-info-bg text-forensics-info-text',
  hypothesis: 'bg-forensics-info-bg text-forensics-info-text',
  finding: 'bg-forensics-warning-bg text-forensics-warning-text',
  actionItem: 'bg-orange-50 text-orange-700',
  conclusion: 'bg-forensics-success-bg text-forensics-success-text',
};

export const STATUS_BADGE: Record<NotebookEntryStatus, string> = {
  draft: 'bg-forensics-panel text-forensics-muted',
  reviewed: 'bg-forensics-warning-bg text-forensics-warning-text',
  final: 'bg-forensics-success-bg text-forensics-success-text',
};

export const STATUS_LABEL: Record<NotebookEntryStatus, string> = {
  draft: '草稿',
  reviewed: '审核中',
  final: '定稿',
};

export const NODE_TYPE_BADGE: Record<string, string> = {
  File: 'bg-forensics-info-bg text-forensics-info-text',
  Artifact: 'bg-forensics-info-bg text-forensics-info-text',
  TimelineEvent: 'bg-forensics-warning-bg text-forensics-warning-text',
  Entity: 'bg-forensics-success-bg text-forensics-success-text',
  Lead: 'bg-forensics-error-bg text-forensics-error-text',
  NotebookEntry: 'bg-forensics-panel text-forensics-muted',
};

export function formatTimestampShort(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

export function simpleMarkdownToHtml(md: string): string {
  const escaped = md
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  const html = escaped
    .replace(/^### (.+)$/gm, '<h4 class="text-[13px] font-light mt-3 mb-1">$1</h4>')
    .replace(/^## (.+)$/gm, '<h3 class="text-[14px] font-light mt-3 mb-1">$1</h3>')
    .replace(/^# (.+)$/gm, '<h2 class="text-[15px] font-light mt-3 mb-1">$1</h2>')
    .replace(/^- \[x\] (.+)$/gm, '<label class="text-[11px] text-forensics-text-tertiary"><input type="checkbox" checked disabled class="mr-1" />$1</label>')
    .replace(/^- \[ \] (.+)$/gm, '<label class="text-[11px] text-forensics-text-tertiary"><input type="checkbox" disabled class="mr-1" />$1</label>')
    .replace(/^\* (.+)$/gm, '<li class="text-[11px] text-forensics-text-tertiary ml-4">$1</li>')
    .replace(/^(\d+)\. (.+)$/gm, '<li class="text-[11px] text-forensics-text-tertiary ml-4">$1. $2</li>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code class="font-mono text-[11px] bg-forensics-panel-strong px-1 rounded-none">$1</code>')
    .replace(/^> (.+)$/gm, '<blockquote class="border-b border-forensics-border-strong pb-1 my-1 text-[11px] text-forensics-muted">$1</blockquote>')
    .replace(/\n\n/g, '<br/><br/>')
    .replace(/\n/g, '<br/>');
  return html;
}
