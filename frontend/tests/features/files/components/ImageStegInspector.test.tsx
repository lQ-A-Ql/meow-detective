import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ImageStegInspector } from '@/features/files/components/ImageStegInspector';
import type { ImageStegModel } from '@/features/files/hooks/use-image-steg';
import type { ImageStegReport } from '@/types/models';

function model(overrides: Partial<ImageStegModel> = {}): ImageStegModel {
  return { running: false, run: vi.fn(), ...overrides };
}

function completedReport(overrides: Partial<ImageStegReport> = {}): ImageStegReport {
  return {
    status: 'completed',
    format: 'png',
    width: 2,
    height: 1,
    dominantPlane: 'r',
    global: {
      score: 0.82,
      band: 'high',
      methodsUsed: 3,
      methods: [
        { method: 'chiSquare', estimate: 0.9, score: 0.86, samples: 6 },
        { method: 'rs', estimate: 0.78, score: 0.8, samples: 6 },
        { method: 'spa', score: 0.79, samples: 6 },
      ],
    },
    planes: [
      {
        plane: 'r',
        samples: 6,
        uniqueValues: 4,
        assessment: { score: 0.82, band: 'high', methodsUsed: 3, methods: [] },
      },
    ],
    blocks: [{ x: 0, y: 0, width: 2, height: 1, plane: 'r', score: 0.77, band: 'high', methodsUsed: 3 }],
    blocksScored: 1,
    blocksTotal: 1,
    warnings: [
      { code: 'image.steg.blocks-truncated', severity: 'info', message: 'Block budget exceeded.' },
      { code: 'image.steg.unknown-code', severity: 'warning', message: 'Backend fallback text.' },
    ],
    ...overrides,
  };
}

describe('ImageStegInspector', () => {
  it('starts idle and runs only when the button is pressed', () => {
    const run = vi.fn();
    render(<ImageStegInspector model={model({ run })} />);
    expect(screen.getByText('对文件浏览器中选中的 PNG/BMP 文件执行只读统计 LSB 隐写分诊。')).toBeInTheDocument();
    expect(screen.getByText('仅会话展示：结果不会写入案件。')).toBeInTheDocument();
    const button = screen.getByRole('button', { name: '运行隐写分析' });
    expect(button).toHaveAttribute('data-shape', 'circle');
    expect(button.textContent).toBe('');
    expect(button.querySelector('svg')).not.toBeNull();
    button.click();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('disables the trigger while the analysis is running', () => {
    render(<ImageStegInspector model={model({ running: true })} />);
    const button = screen.getByRole('button', { name: '正在分析像素统计…' });
    expect(button).toHaveAttribute('data-shape', 'circle');
    expect(button).toBeDisabled();
  });

  it('renders fused score, planes, hotspots and localized warnings', () => {
    render(<ImageStegInspector model={model({ report: completedReport() })} />);
    expect(screen.getByText('已完成')).toBeInTheDocument();
    expect(screen.getByText('png')).toBeInTheDocument();
    expect(screen.getByText('2 × 1')).toBeInTheDocument();
    expect(screen.getByText('整图评估')).toBeInTheDocument();
    expect(screen.getByText('融合评分')).toBeInTheDocument();
    expect(screen.getAllByText(/82\.0%/).length).toBeGreaterThan(0);
    expect(screen.getByText('卡方')).toBeInTheDocument();
    expect(screen.getByText(/6 个样本 · 4 个唯一值/)).toBeInTheDocument();
    expect(screen.getByText('0,0 · 2×1 · 77.0%')).toBeInTheDocument();
    expect(screen.getByText('已评分 1 / 1 个块')).toBeInTheDocument();
    expect(screen.getByText('块数量超出预算，已按固定步长抽样。')).toBeInTheDocument();
    expect(screen.getByText('Backend fallback text.')).toBeInTheDocument();
  });

  it('renders a completed report whose optional arrays are omitted from the wire payload', () => {
    render(
      <ImageStegInspector
        model={model({
          report: {
            status: 'completed',
            format: 'bmp',
            width: 2,
            height: 1,
            global: { score: 0.5, band: 'medium', methodsUsed: 1 },
            blocksScored: 0,
            blocksTotal: 0,
          },
        })}
      />,
    );
    expect(screen.getByText('已完成')).toBeInTheDocument();
    expect(screen.getByText('bmp')).toBeInTheDocument();
    expect(screen.getByText('2 × 1')).toBeInTheDocument();
    expect(screen.getByText('整图评估')).toBeInTheDocument();
    expect(screen.getByText('融合评分')).toBeInTheDocument();
    expect(screen.getByText('50.0% · 中')).toBeInTheDocument();
  });

  it('renders a status-only unsupported report without optional arrays', () => {
    render(
      <ImageStegInspector
        model={model({
          report: { status: 'unsupported', reason: 'image.format', blocksScored: 0, blocksTotal: 0 },
        })}
      />,
    );
    expect(screen.getByText('不支持的格式')).toBeInTheDocument();
    expect(screen.getByText('文件魔数既不是 PNG 也不是 BMP。')).toBeInTheDocument();
    expect(screen.queryByText('整图评估')).not.toBeInTheDocument();
  });

  it('renders status-only failures with a localized reason', () => {
    render(
      <ImageStegInspector
        model={model({
          report: {
            status: 'unsupported',
            reason: 'image.format',
            planes: [],
            blocks: [],
            blocksScored: 0,
            blocksTotal: 0,
            warnings: [],
          },
        })}
      />,
    );
    expect(screen.getByText('不支持的格式')).toBeInTheDocument();
    expect(screen.getByText('文件魔数既不是 PNG 也不是 BMP。')).toBeInTheDocument();
    expect(screen.queryByText('整图评估')).not.toBeInTheDocument();
  });

  it('renders a retry action for read failures', () => {
    const run = vi.fn();
    render(<ImageStegInspector model={model({ errorText: '分析失败', run })} />);
    expect(screen.getByText('分析失败')).toBeInTheDocument();
    screen.getByRole('button', { name: '重试' }).click();
    expect(run).toHaveBeenCalledTimes(1);
  });
});
