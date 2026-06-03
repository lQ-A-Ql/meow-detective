import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ImageViewer } from './ImageViewer';

describe('ImageViewer', () => {
  it('shows loading state', () => {
    render(<ImageViewer src="data:image/png;base64,abc" />);

    expect(screen.getByText('加载中...')).toBeDefined();
  });

  it('shows error state when image fails to load', () => {
    const { container } = render(<ImageViewer src="data:image/png;base64,abc" />);

    const img = container.querySelector('img');
    expect(img).toBeTruthy();

    fireEvent.error(img!);

    expect(screen.getByText('图片格式不支持或文件损坏')).toBeDefined();
  });
});
