import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { VideoViewer } from './VideoViewer';

describe('VideoViewer', () => {
  it('renders with a mock src prop', () => {
    render(<VideoViewer src="evidence-media://handle/dGVzdA" />);

    const video = document.querySelector('video');
    expect(video).toBeTruthy();
    expect(video!.src).toContain('evidence-media://handle/dGVzdA');
  });

  it('shows loading state initially', () => {
    render(<VideoViewer src="evidence-media://handle/dGVzdA" />);

    expect(screen.getByText('加载中...')).toBeDefined();
  });

  it('displays mime type in status bar', () => {
    render(
      <VideoViewer
        src="evidence-media://handle/dGVzdA"
        mimeType="video/mp4"
      />,
    );

    expect(screen.getByText('video/mp4')).toBeDefined();
  });

  it('displays file name when provided', () => {
    render(
      <VideoViewer
        src="evidence-media://handle/dGVzdA"
        fileName="sample.mp4"
      />,
    );

    expect(screen.getByText('sample.mp4')).toBeDefined();
  });
});
