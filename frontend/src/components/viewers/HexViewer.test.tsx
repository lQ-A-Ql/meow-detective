import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HexViewer } from './HexViewer';

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe = vi.fn();
    disconnect = vi.fn();
    unobserve = vi.fn();
  });
});

describe('HexViewer', () => {
  it('renders hex lines', () => {
    const lines = [
      '00000000  4D 5A 90 00 03 00 00 00  04 00 00 00 FF FF 00 00',
      '00000010  B8 00 00 00 00 00 00 00  40 00 00 00 00 00 00 00',
    ];

    render(<HexViewer lines={lines} />);

    expect(screen.getByText('00000000')).toBeDefined();
    expect(screen.getByText('00000010')).toBeDefined();
    expect(screen.getByText('4D')).toBeDefined();
    expect(screen.getByText('5A')).toBeDefined();
  });

  it('shows empty state when no data', () => {
    render(<HexViewer lines={[]} />);

    expect(screen.getByText('选择文件后显示十六进制预览')).toBeDefined();
  });
});
