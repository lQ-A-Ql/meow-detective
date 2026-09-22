import type {
  NotebookEntryStatus,
  NotebookEntryType,
} from '@/types/models';
import type { ReactNode } from 'react';
import { Checkbox } from '@/app/components/ui/checkbox';

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

export function simpleMarkdownToReact(md: string): ReactNode[] {
  return md.split(/\r?\n/).map((line, index) => {
    const key = `${index}-${line}`;
    if (!line.trim()) return <br key={key} />;
    const heading = /^(#{1,3}) (.+)$/.exec(line);
    if (heading) {
      const Heading = heading[1].length === 1 ? 'h2' : heading[1].length === 2 ? 'h3' : 'h4';
      return <Heading key={key} className="mt-3 mb-1 font-light">{renderMarkdownInline(heading[2])}</Heading>;
    }
    const checklist = /^- \[([ xX])\] (.+)$/.exec(line);
    if (checklist) {
      return <div key={key} className="flex items-center gap-1 text-[11px] text-forensics-text-tertiary"><Checkbox checked={checklist[1].toLowerCase() === 'x'} disabled variant="forensics" checkboxSize="compact" />{renderMarkdownInline(checklist[2])}</div>;
    }
    const bullet = /^\* (.+)$/.exec(line);
    if (bullet) return <div key={key} className="ml-4 text-[11px] text-forensics-text-tertiary">• {renderMarkdownInline(bullet[1])}</div>;
    const ordered = /^(\d+)\. (.+)$/.exec(line);
    if (ordered) return <div key={key} className="ml-4 text-[11px] text-forensics-text-tertiary">{ordered[1]}. {renderMarkdownInline(ordered[2])}</div>;
    if (line.startsWith('> ')) return <blockquote key={key} className="my-1 border-b border-forensics-border-strong pb-1 text-[11px] text-forensics-muted">{renderMarkdownInline(line.slice(2))}</blockquote>;
    return <p key={key} className="m-0">{renderMarkdownInline(line)}</p>;
  });
}

function renderMarkdownInline(value: string): ReactNode[] {
  return value.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean).map((part, index) => {
    if (part.startsWith('**') && part.endsWith('**')) return <strong key={index}>{part.slice(2, -2)}</strong>;
    if (part.startsWith('`') && part.endsWith('`')) return <code key={index} className="bg-forensics-panel-strong px-1 font-mono text-[11px]">{part.slice(1, -1)}</code>;
    return <span key={index}>{part}</span>;
  });
}
