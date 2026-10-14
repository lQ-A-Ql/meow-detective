import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { NtfsTechnicalInspector } from '@/features/files/components/NtfsTechnicalInspector';
import { ntfsInspectorRuns } from '@/features/files/ntfs-inspector-runs';
import { ntfsTechnicalFile } from '../support/ntfs-technical';

describe('NTFS technical presentation', () => {
  it('shows loading only for eligible objects', () => {
    const view = render(<NtfsTechnicalInspector eligible loading />);
    expect(screen.getByText('正在读取 NTFS 技术结构…')).toBeInTheDocument();
    view.rerender(<NtfsTechnicalInspector eligible={false} loading />);
    expect(screen.queryByText('NTFS 技术结构')).not.toBeInTheDocument();
  });
  it('shows errors and delegates retry without owning IO', () => {
    const retry = vi.fn();
    render(<NtfsTechnicalInspector eligible errorText="读取失败" onRetry={retry} />);
    expect(screen.getByRole('alert')).toHaveTextContent('读取失败');
    fireEvent.click(screen.getByRole('button', { name: '重试读取' }));
    expect(retry).toHaveBeenCalledTimes(1);
  });
  it('makes unsupported capability explicit', () => {
    render(<NtfsTechnicalInspector eligible unsupported errorText="Not image-backed" onRetry={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent('当前对象不支持 NTFS 技术检查');
    expect(screen.getByText('Not image-backed')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '重试读取' })).not.toBeInTheDocument();
  });
  it('reports the bounded structure display without implying completeness', () => {
    const technical = ntfsTechnicalFile(33);
    render(<NtfsTechnicalInspector eligible technical={technical} runs={ntfsInspectorRuns(technical)} />);
    expect(screen.getByText('已显示 32 / 33 个数据运行')).toBeInTheDocument();
    expect(screen.getAllByText('原始字节 11 01 02')).toHaveLength(32);
  });
});
