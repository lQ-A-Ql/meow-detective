import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AudioViewer } from './AudioViewer';

describe('AudioViewer', () => {
  it('renders with a mock src prop', () => {
    render(<AudioViewer src="evidence-media://handle/dGVzdA" />);

    const audio = document.querySelector('audio');
    expect(audio).toBeTruthy();
    expect(audio!.src).toContain('evidence-media://handle/dGVzdA');
  });

  it('shows loading state initially', () => {
    render(<AudioViewer src="evidence-media://handle/dGVzdA" />);

    expect(screen.getByText('加载中...')).toBeDefined();
  });

  it('displays mime type in info section', () => {
    render(
      <AudioViewer
        src="evidence-media://handle/dGVzdA"
        mimeType="audio/mpeg"
      />,
    );

    expect(screen.getByText('audio/mpeg')).toBeDefined();
  });

  it('displays file name when provided', () => {
    render(
      <AudioViewer
        src="evidence-media://handle/dGVzdA"
        fileName="recording.mp3"
      />,
    );

    expect(screen.getByText('recording.mp3')).toBeDefined();
  });
});
