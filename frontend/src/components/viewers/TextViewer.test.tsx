import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TextViewer } from './TextViewer';

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe = vi.fn();
    disconnect = vi.fn();
    unobserve = vi.fn();
  });
});

describe('TextViewer', () => {
  it('renders text lines', () => {
    const content = ['Hello World', 'Second line', 'Third line'].join('\n');
    render(<TextViewer content={content} encoding="UTF-8" />);

    expect(screen.getByText('Hello World')).toBeDefined();
    expect(screen.getByText('Second line')).toBeDefined();
    expect(screen.getByText('Third line')).toBeDefined();
  });

  it('renders when ResizeObserver is unavailable', () => {
    vi.stubGlobal('ResizeObserver', undefined);

    render(<TextViewer content="Fallback text" encoding="UTF-8" />);

    expect(screen.getByText('Fallback text')).toBeDefined();
  });

  it('shows line numbers', () => {
    const content = ['Alpha', 'Beta', 'Gamma'].join('\n');
    render(<TextViewer content={content} encoding="UTF-8" />);

    const lineNumberDivs = screen.getAllByTestId('text-line-number');
    expect(lineNumberDivs.length).toBe(3);
    expect(lineNumberDivs[0].textContent?.trim()).toBe('1');
    expect(lineNumberDivs[1].textContent?.trim()).toBe('2');
    expect(lineNumberDivs[2].textContent?.trim()).toBe('3');
  });

  it('paginates large text inputs to 1000 visible lines at a time', () => {
    const content = Array.from({ length: 2505 }, (_, index) => `Line ${index + 1}`).join('\n');
    render(<TextViewer content={content} encoding="UTF-8" />);

    expect(screen.getByRole('status').textContent).toContain('大内容模式');

    const lineNumberDivs = screen.getAllByTestId('text-line-number');
    expect(lineNumberDivs.length).toBeLessThan(1000);
    expect(lineNumberDivs[0].textContent?.trim()).toBe('1');
    expect(screen.getByText('1+')).toBeDefined();
    expect(screen.queryByText('Line 2505')).toBeNull();

    const scrollContainer = screen.getByTestId('text-scroll-container');
    fireEvent.scroll(scrollContainer, { target: { scrollTop: 17_640 } });

    const scrolledLineNumbers = screen.getAllByTestId('text-line-number');
    expect(scrolledLineNumbers.length).toBeLessThan(1000);
    expect(scrolledLineNumbers[0].textContent?.trim()).toBe('973');
    expect(screen.getByText('Line 981')).toBeDefined();
    expect(scrolledLineNumbers[0].getAttribute('data-line-number')).not.toBe('1');

    const nextButton = screen.getAllByRole('button')[1];
    fireEvent.click(nextButton);

    const secondPageLineNumbers = screen.getAllByTestId('text-line-number');
    expect(secondPageLineNumbers.length).toBeLessThan(1000);
    expect(secondPageLineNumbers[0].textContent?.trim()).toBe('1001');
    expect(screen.getByText('2+')).toBeDefined();
  });

  it('segments a long logical line before rendering it', () => {
    render(<TextViewer content={'A'.repeat(20_000)} encoding="UTF-8" />);

    const renderedSegments = screen.getAllByTestId('text-line-content');
    expect(renderedSegments.length).toBeGreaterThan(1);
    expect(renderedSegments.every((segment) => (segment.textContent?.length ?? 0) <= 8 * 1024)).toBe(true);
    expect(renderedSegments.every((segment) => segment.parentElement?.className.includes('whitespace-pre'))).toBe(true);
    expect(renderedSegments.every((segment) => !segment.parentElement?.className.includes('whitespace-pre-wrap'))).toBe(true);
    expect(screen.queryByText('A'.repeat(20_000))).toBeNull();
  });

  it('keeps the exact logical line count without allocating a line array', () => {
    render(<TextViewer content={'first\nsecond\nthird'} encoding="UTF-8" />);

    expect(screen.getByText('3 行')).toBeDefined();
  });
});
