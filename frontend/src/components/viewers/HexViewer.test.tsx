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
    expect(screen.getByTestId('hex-byte-0')).toHaveTextContent('4D');
    expect(screen.getByTestId('hex-byte-15')).toHaveTextContent('00');
    expect(screen.getByTestId('ascii-byte-0')).toHaveTextContent('M');
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
    expect(screen.getByTestId('hex-byte-0')).toHaveTextContent('4D');
  });

  it('renders only the visible window for large hex datasets', () => {
    const lines = Array.from({ length: 1000 }, (_, index) => {
      const offset = index.toString(16).padStart(8, '0').toUpperCase();
      return `${offset}  41 42 43 44 45 46 47 48  49 4A 4B 4C 4D 4E 4F 50`;
    });

    render(<HexViewer lines={lines} lineHeight={20} />);

    const scrollContainer = screen.getByTestId('hex-scroll-container');
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

  it('requests more data when scrolling near the bottom edge', () => {
    const lines = Array.from({ length: 1000 }, (_, index) => {
      const offset = (index * 16).toString(16).padStart(8, '0').toUpperCase();
      return `${offset}  41 42 43 44 45 46 47 48  49 4A 4B 4C 4D 4E 4F 50`;
    });
    const onNeedMoreRange = vi.fn();

    render(
      <HexViewer
        lines={lines}
        lineHeight={20}
        onNeedMoreRange={onNeedMoreRange}
      />,
    );

    const scrollContainer = screen.getByTestId('hex-scroll-container');
    fireEvent.scroll(scrollContainer, {
      target: { scrollTop: 19_500 },
    });

    expect(onNeedMoreRange).toHaveBeenCalledWith('next');
  });

  it('does not request a previous range for the initial chunk', () => {
    const onNeedMoreRange = vi.fn();
    const rawBytes = Array.from({ length: 64 * 1024 }, (_, index) => index % 256);

    render(
      <HexViewer
        lines={[]}
        rawBytes={rawBytes}
        baseOffset={0}
        fileSize={128 * 1024}
        onNeedMoreRange={onNeedMoreRange}
      />,
    );

    fireEvent.scroll(screen.getByTestId('hex-scroll-container'), {
      target: { scrollTop: 0 },
    });

    expect(onNeedMoreRange).not.toHaveBeenCalled();
  });

  it('requests a previous range from a later chunk', () => {
    const onNeedMoreRange = vi.fn();
    const rawBytes = Array.from({ length: 64 * 1024 }, (_, index) => index % 256);

    render(
      <HexViewer
        lines={[]}
        rawBytes={rawBytes}
        baseOffset={64 * 1024}
        fileSize={128 * 1024}
        onNeedMoreRange={onNeedMoreRange}
      />,
    );

    fireEvent.scroll(screen.getByTestId('hex-scroll-container'), {
      target: { scrollTop: 0 },
    });

    expect(onNeedMoreRange).toHaveBeenCalledWith('previous');
  });

  it('links Hex and ASCII highlighting by absolute byte offset', () => {
    render(
      <HexViewer
        lines={[]}
        rawBytes={[0x41, 0x42, 0x00]}
        baseOffset={32}
        fileSize={35}
      />,
    );

    const hex = screen.getByTestId('hex-byte-33');
    const ascii = screen.getByTestId('ascii-byte-33');
    fireEvent.pointerMove(hex);

    expect(hex).toHaveAttribute('data-highlighted', 'true');
    expect(ascii).toHaveAttribute('data-highlighted', 'true');
    expect(ascii).toHaveTextContent('B');

    fireEvent.pointerMove(screen.getByTestId('ascii-byte-34'));
    expect(screen.getByTestId('hex-byte-34')).toHaveAttribute('data-highlighted', 'true');
    expect(screen.getByTestId('ascii-byte-34')).toHaveTextContent('.');
    expect(hex).not.toHaveAttribute('data-highlighted');
  });

  it('locks linked highlighting on pointer selection and supports keyboard byte navigation', () => {
    render(<HexViewer lines={[]} rawBytes={[0x41, 0x42, 0x43]} baseOffset={16} />);

    const container = screen.getByTestId('hex-scroll-container');
    fireEvent.pointerDown(screen.getByTestId('ascii-byte-16'), { button: 0, pointerId: 1 });
    fireEvent.pointerUp(screen.getByTestId('ascii-byte-16'), { pointerId: 1 });
    fireEvent.pointerLeave(container);

    expect(screen.getByTestId('hex-byte-16')).toHaveAttribute('data-selected', 'true');
    expect(screen.getByTestId('ascii-byte-16')).toHaveAttribute('data-selected', 'true');

    fireEvent.keyDown(container, { key: 'ArrowRight' });
    expect(screen.getByTestId('hex-byte-17')).toHaveAttribute('data-selected', 'true');
    expect(screen.getByTestId('ascii-byte-17')).toHaveAttribute('data-selected', 'true');
    expect(screen.getByTestId('hex-byte-16')).not.toHaveAttribute('data-selected');
  });

  it('does not jump highlighting to the active first byte over cell gaps', () => {
    render(
      <HexViewer
        lines={[]}
        rawBytes={[0x41, 0x42, 0x43]}
        baseOffset={64}
        activeOffset={64}
      />,
    );

    const container = screen.getByTestId('hex-scroll-container');
    fireEvent.pointerMove(screen.getByTestId('hex-byte-66'));
    fireEvent.pointerMove(container);

    expect(screen.getByTestId('hex-byte-66')).toHaveAttribute('data-highlighted', 'true');
    expect(screen.getByTestId('ascii-byte-66')).toHaveAttribute('data-highlighted', 'true');
    expect(screen.getByTestId('hex-byte-64')).not.toHaveAttribute('data-highlighted');

    fireEvent.pointerLeave(container);
    expect(screen.getByTestId('hex-byte-66')).not.toHaveAttribute('data-highlighted');
    expect(screen.getByTestId('hex-byte-64')).not.toHaveAttribute('data-highlighted');
  });

  it('selects a linked Hex and ASCII byte range by plain pointer drag', () => {
    render(
      <HexViewer
        lines={[]}
        rawBytes={[0x41, 0x42, 0x43, 0x44, 0x45, 0x46]}
        baseOffset={16}
      />,
    );

    fireEvent.pointerDown(screen.getByTestId('hex-byte-17'), { button: 0, pointerId: 2 });
    fireEvent.pointerMove(screen.getByTestId('ascii-byte-20'), { buttons: 1, pointerId: 2 });
    fireEvent.pointerUp(screen.getByTestId('ascii-byte-20'), { pointerId: 2 });

    for (const offset of [17, 18, 19, 20]) {
      expect(screen.getByTestId(`hex-byte-${offset}`)).toHaveAttribute('data-selected', 'true');
      expect(screen.getByTestId(`ascii-byte-${offset}`)).toHaveAttribute('data-selected', 'true');
    }
    expect(screen.getByTestId('hex-byte-16')).not.toHaveAttribute('data-selected');
    expect(screen.getByTestId('ascii-byte-21')).not.toHaveAttribute('data-selected');
  });

  it('ends plain pointer selection when the pointer is released outside the viewer', () => {
    render(
      <HexViewer
        lines={[]}
        rawBytes={[0x41, 0x42, 0x43, 0x44]}
        baseOffset={32}
      />,
    );

    fireEvent.pointerDown(screen.getByTestId('hex-byte-32'), { button: 0, pointerId: 3 });
    fireEvent.pointerMove(screen.getByTestId('hex-byte-34'), { buttons: 1, pointerId: 3 });
    fireEvent.pointerUp(window, { pointerId: 3 });
    fireEvent.pointerMove(screen.getByTestId('hex-byte-35'), { buttons: 0, pointerId: 3 });

    expect(screen.getByTestId('hex-byte-34')).toHaveAttribute('data-selected', 'true');
    expect(screen.getByTestId('hex-byte-35')).not.toHaveAttribute('data-selected');
  });

  it('ends plain pointer selection when the window loses focus', () => {
    render(
      <HexViewer
        lines={[]}
        rawBytes={[0x41, 0x42, 0x43, 0x44]}
        baseOffset={48}
      />,
    );

    fireEvent.pointerDown(screen.getByTestId('hex-byte-48'), { button: 0, pointerId: 4 });
    fireEvent.pointerMove(screen.getByTestId('hex-byte-50'), { buttons: 1, pointerId: 4 });
    fireEvent.blur(window);
    fireEvent.pointerMove(screen.getByTestId('hex-byte-51'), { buttons: 0, pointerId: 4 });

    expect(screen.getByTestId('hex-byte-50')).toHaveAttribute('data-selected', 'true');
    expect(screen.getByTestId('hex-byte-51')).not.toHaveAttribute('data-selected');
  });

  it('preserves selection when only the compatibility lines wrapper changes', () => {
    const rawBytes = [0x41, 0x42, 0x43];
    const { rerender } = render(
      <HexViewer lines={['first wrapper']} rawBytes={rawBytes} baseOffset={80} />,
    );

    fireEvent.pointerDown(screen.getByTestId('hex-byte-81'), { button: 0, pointerId: 5 });
    fireEvent.pointerUp(window, { pointerId: 5 });
    rerender(<HexViewer lines={['recreated wrapper']} rawBytes={rawBytes} baseOffset={80} />);

    expect(screen.getByTestId('hex-byte-81')).toHaveAttribute('data-selected', 'true');
    expect(screen.getByTestId('ascii-byte-81')).toHaveAttribute('data-selected', 'true');
  });
});
