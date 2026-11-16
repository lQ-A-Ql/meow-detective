import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ImageMetadataPanel } from '@/components/preview/ImageMetadataPanel';
import type { ImageMetadata } from '@/types/viewer';

const structure = {
  declaredEnd: 0x200,
  trailingBytes: 64,
  truncated: true,
  segmentCount: 3,
  segments: [
    { kind: 'png.ihdr', offset: 0, length: 13 },
    { kind: 'png.idat', offset: 33, length: 128 },
    { kind: 'png.iend', offset: 161, length: 12 },
  ],
  segmentsTruncated: true,
  findings: [
    { code: 'image.truncated', severity: 'error' as const, offset: 0x200 },
    { code: 'image.trailing-data', severity: 'warning' as const, offset: 0x240 },
    { code: 'image.segment-limit', severity: 'info' as const },
  ],
  hashes: {
    wholeFileSha256: 'a'.repeat(64),
    trailingSha256: 'b'.repeat(64),
  },
};

function metadata(overrides: Partial<ImageMetadata>): ImageMetadata {
  return { status: 'present', warnings: [], exifBlocks: [], ...overrides };
}

describe('ImageMetadataPanel', () => {
  it('keeps status, format, dimensions and structure visible when corrupt', () => {
    render(
      <ImageMetadataPanel
        fileSize={4096}
        metadata={metadata({
          status: 'corrupt',
          format: 'png',
          width: 640,
          height: 480,
          warnings: [{ code: 'image.truncated', severity: 'error', message: 'truncated container', offset: 0x200 }],
          structure,
        })}
      />,
    );

    expect(screen.getByText('结构损坏')).toBeInTheDocument();
    expect(screen.getByText('png')).toBeInTheDocument();
    expect(screen.getByText('640 × 480')).toBeInTheDocument();
    expect(screen.getByText('4.0 KB')).toBeInTheDocument();
    expect(screen.getByText('容器结构')).toBeInTheDocument();
    expect(screen.getAllByText('容器在声明结束位置之前被截断。').length).toBeGreaterThan(0);
    expect(screen.getAllByText('image.truncated').length).toBeGreaterThan(0);
    expect(screen.getByText('段列表已按分析上限截断。')).toBeInTheDocument();
  });

  it('renders multiple EXIF blocks with their own warnings', () => {
    render(
      <ImageMetadataPanel
        metadata={metadata({
          exifBlocks: [
            { source: 'exif.app1', offset: 12, length: 128, decoded: true, fieldCount: 4, warnings: [] },
            {
              source: 'xmp',
              offset: 200,
              length: 64,
              decoded: false,
              fieldCount: 0,
              warnings: [{ code: 'image.exif.decode-failed', severity: 'warning', message: 'decode failed' }],
            },
          ],
        })}
      />,
    );

    expect(screen.getByText(/exif\.app1/)).toBeInTheDocument();
    expect(screen.getByText(/xmp/)).toBeInTheDocument();
  });

  it('colors structure findings by severity', () => {
    const { container } = render(<ImageMetadataPanel metadata={metadata({ structure })} />);

    expect(container.querySelector('[data-severity="error"]')).toHaveClass('text-forensics-error-text');
    expect(container.querySelector('[data-severity="warning"]')).toHaveClass('text-forensics-warning-text');
    expect(container.querySelector('[data-severity="info"]')).toHaveClass('text-forensics-muted-light');
  });

  it('copies hashes and reports the copied state', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });

    render(<ImageMetadataPanel metadata={metadata({ structure })} />);

    fireEvent.click(screen.getAllByRole('button', { name: '复制哈希' })[0]);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('a'.repeat(64)));
    expect(await screen.findByRole('button', { name: '已复制' })).toBeInTheDocument();
  });

  it('shows the metadata-only hint when requested', () => {
    render(<ImageMetadataPanel metadataOnly metadata={metadata({ format: 'tiff' })} />);
    expect(screen.getByText('仅元数据图片')).toBeInTheDocument();
    expect(screen.getByText('TIFF 仅支持元数据解析，浏览器不支持内联预览。')).toBeInTheDocument();
  });
});
