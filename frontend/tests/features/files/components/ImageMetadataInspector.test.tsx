import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ImageMetadataInspector } from '@/features/files/components/ImageMetadataInspector';

describe('ImageMetadataInspector', () => {
  it('renders present EXIF fields', () => {
    render(
      <ImageMetadataInspector
        loading={false}
        metadata={{ status: 'present', format: 'jpeg', width: 1920, height: 1080, latitude: 31.2, longitude: 121.4 }}
      />,
    );
    expect(screen.getByText('已发现')).toBeInTheDocument();
    expect(screen.getByText('jpeg')).toBeInTheDocument();
    expect(screen.getByText('1920 × 1080')).toBeInTheDocument();
    expect(screen.getByText('31.2')).toBeInTheDocument();
  });

  it('renders absent and corrupt states without fabricating fields', () => {
    const { rerender } = render(<ImageMetadataInspector loading={false} metadata={{ status: 'absent' }} />);
    expect(screen.getAllByText('无 EXIF').length).toBeGreaterThan(0);

    rerender(<ImageMetadataInspector loading={false} metadata={{ status: 'corrupt', format: 'jpeg' }} />);
    expect(screen.getAllByText('结构损坏').length).toBeGreaterThan(0);
  });

  it('renders loading and retryable error states', () => {
    const retry = vi.fn();
    const { rerender } = render(<ImageMetadataInspector loading metadata={null} />);
    expect(screen.getByText('正在读取图片元数据…')).toBeInTheDocument();
    rerender(<ImageMetadataInspector loading={false} error={{ code: 'E', message: '读取失败', category: 'io', recoverable: true }} onRetry={retry} />);
    expect(screen.getByText('读取失败')).toBeInTheDocument();
    screen.getByRole('button', { name: '重试' }).click();
    expect(retry).toHaveBeenCalledTimes(1);
  });
});
