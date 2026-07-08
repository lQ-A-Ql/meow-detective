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
  observation: 'bg-purple-50 text-purple-700',
  hypothesis: 'bg-blue-50 text-blue-700',
  finding: 'bg-amber-50 text-amber-700',
  actionItem: 'bg-orange-50 text-orange-700',
  conclusion: 'bg-green-50 text-green-700',
};

export const STATUS_BADGE: Record<NotebookEntryStatus, string> = {
  draft: 'bg-gray-100 text-gray-600',
  reviewed: 'bg-amber-50 text-amber-700',
  final: 'bg-green-50 text-green-700',
};

export const STATUS_LABEL: Record<NotebookEntryStatus, string> = {
  draft: '草稿',
  reviewed: '审核中',
  final: '定稿',
};

export const NODE_TYPE_BADGE: Record<string, string> = {
  File: 'bg-blue-50 text-blue-700',
  Artifact: 'bg-purple-50 text-purple-700',
  TimelineEvent: 'bg-amber-50 text-amber-700',
  Entity: 'bg-green-50 text-green-700',
  Lead: 'bg-red-50 text-red-700',
  NotebookEntry: 'bg-gray-50 text-gray-700',
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
    .replace(/^### (.+)$/gm, '<h4 class="text-[13px] font-semibold mt-3 mb-1">$1</h4>')
    .replace(/^## (.+)$/gm, '<h3 class="text-[14px] font-semibold mt-3 mb-1">$1</h3>')
    .replace(/^# (.+)$/gm, '<h2 class="text-[15px] font-semibold mt-3 mb-1">$1</h2>')
    .replace(/^- \[x\] (.+)$/gm, '<label class="text-[11px] text-[#555]"><input type="checkbox" checked disabled class="mr-1" />$1</label>')
    .replace(/^- \[ \] (.+)$/gm, '<label class="text-[11px] text-[#555]"><input type="checkbox" disabled class="mr-1" />$1</label>')
    .replace(/^\* (.+)$/gm, '<li class="text-[11px] text-[#555] ml-4">$1</li>')
    .replace(/^(\d+)\. (.+)$/gm, '<li class="text-[11px] text-[#555] ml-4">$1. $2</li>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code class="font-mono text-[11px] bg-[#f0f0f0] px-1 rounded">$1</code>')
    .replace(/^> (.+)$/gm, '<blockquote class="border-l-2 border-[#ccc] pl-3 my-1 text-[11px] text-[#777]">$1</blockquote>')
    .replace(/\n\n/g, '<br/><br/>')
    .replace(/\n/g, '<br/>');
  return html;
}
