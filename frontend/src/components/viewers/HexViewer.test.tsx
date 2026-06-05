import { fireEvent, render, screen } from '@testing-library/react';
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

  it('renders when ResizeObserver is unavailable', () => {
    vi.unstubAllGlobals();

    render(
      <HexViewer
        lines={['00000000  4D 5A 90 00 03 00 00 00  04 00 00 00 FF FF 00 00']}
      />
    );

    expect(screen.getByText('00000000')).toBeDefined();
    expect(screen.getByText('4D')).toBeDefined();
  });

  it('renders only the visible window for large hex datasets', () => {
    const lines = Array.from({ length: 1000 }, (_, index) => {
      const offset = index.toString(16).padStart(8, '0').toUpperCase();
      return `${offset}  41 42 43 44 45 46 47 48  49 4A 4B 4C 4D 4E 4F 50`;
    });

    const { container } = render(<HexViewer lines={lines} lineHeight={20} />);

    expect(screen.getByRole('status').textContent).toContain('大内容模式');

    const scrollContainer = container.querySelector('.overflow-auto') as HTMLDivElement;
    const visibleWindow = screen.getByTestId('hex-visible-window');

    expect(visibleWindow.childElementCount).toBeLessThan(lines.length);
    expect(visibleWindow.textContent).toContain('00000000');
    expect(visibleWindow.textContent).not.toContain('000003E7');
    expect(visibleWindow.firstElementChild?.getAttribute('data-row-index')).toBe('0');

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 19_000 } });

    expect(visibleWindow.childElementCount).toBeLessThan(lines.length);
    expect(visibleWindow.textContent).toContain('000003B6');
    expect(visibleWindow.textContent).not.toContain('00000000');
    expect(visibleWindow.firstElementChild?.getAttribute('data-row-index')).toBe('945');
  });
});
