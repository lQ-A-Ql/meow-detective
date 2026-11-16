import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ImageMetadataInspector } from '@/features/files/components/ImageMetadataInspector';
import type { ImageMetadata } from '@/types/models';

function metadata(overrides: Partial<ImageMetadata>): ImageMetadata {
  return { status: 'present', warnings: [], exifBlocks: [], ...overrides };
}

describe('ImageMetadataInspector', () => {
  it('renders present EXIF fields', () => {
    render(
      <ImageMetadataInspector
        loading={false}
        metadata={metadata({
          format: 'jpeg',
          width: 1920,
          height: 1080,
          latitude: 31.2,
          longitude: 121.4,
        })}
      />,
    );
    expect(screen.getByText('已发现')).toBeInTheDocument();
    expect(screen.getByText('jpeg')).toBeInTheDocument();
    expect(screen.getByText('1920 × 1080')).toBeInTheDocument();
    expect(screen.getByText('31.2')).toBeInTheDocument();
  });

  it('renders absent and corrupt states without fabricating fields', () => {
    const { rerender } = render(<ImageMetadataInspector loading={false} metadata={metadata({ status: 'absent' })} />);
    expect(screen.getAllByText('无 EXIF').length).toBeGreaterThan(0);

    rerender(<ImageMetadataInspector loading={false} metadata={metadata({ status: 'corrupt', format: 'jpeg' })} />);
    expect(screen.getAllByText('结构损坏').length).toBeGreaterThan(0);
  });

  it('keeps structure, warnings and hashes visible for corrupt images', () => {
    render(
      <ImageMetadataInspector
        loading={false}
        fileSize={2048}
        metadata={metadata({
          status: 'corrupt',
          format: 'png',
          warnings: [{ code: 'image.truncated', severity: 'error', message: 'truncated', offset: 10 }],
          structure: {
            declaredEnd: 16,
            trailingBytes: 32,
            truncated: true,
            segmentCount: 2,
            segments: [
              { kind: 'png.ihdr', offset: 0, length: 13 },
              { kind: 'png.idat', offset: 33, length: 64 },
            ],
            segmentsTruncated: true,
            findings: [{ code: 'image.truncated', severity: 'warning', offset: 16 }],
            hashes: { wholeFileSha256: 'a'.repeat(64), trailingSha256: 'b'.repeat(64) },
          },
        })}
      />,
    );
    expect(screen.getByText('png')).toBeInTheDocument();
    expect(screen.getByText('2.0 KB')).toBeInTheDocument();
    expect(screen.getByText('容器结构')).toBeInTheDocument();
    expect(screen.getByText('段列表已按分析上限截断。')).toBeInTheDocument();
    expect(screen.getAllByText('image.truncated').length).toBeGreaterThan(0);
    expect(screen.getByText('a'.repeat(64))).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: '复制哈希' }).length).toBeGreaterThan(0);
  });

  it('renders loading and retryable error states', () => {
    const retry = vi.fn();
    const { rerender } = render(<ImageMetadataInspector loading metadata={null} />);
    expect(screen.getByText('正在读取图片元数据…')).toBeInTheDocument();
    rerender(
      <ImageMetadataInspector
        loading={false}
        error={{ code: 'E', message: '读取失败', category: 'io', recoverable: true }}
        onRetry={retry}
      />,
    );
    expect(screen.getByText('读取失败')).toBeInTheDocument();
    screen.getByRole('button', { name: '重试' }).click();
    expect(retry).toHaveBeenCalledTimes(1);
  });
});
