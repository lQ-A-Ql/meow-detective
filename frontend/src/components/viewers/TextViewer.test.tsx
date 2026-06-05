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
    const { container } = render(<TextViewer content={content} encoding="UTF-8" />);

    const lineNumberDivs = container.querySelectorAll('.shrink-0.text-right');
    expect(lineNumberDivs.length).toBe(3);
    expect(lineNumberDivs[0].textContent?.trim()).toBe('1');
    expect(lineNumberDivs[1].textContent?.trim()).toBe('2');
    expect(lineNumberDivs[2].textContent?.trim()).toBe('3');
  });

  it('paginates large text inputs to 1000 visible lines at a time', () => {
    const content = Array.from({ length: 2505 }, (_, index) => `Line ${index + 1}`).join('\n');
    const { container } = render(<TextViewer content={content} encoding="UTF-8" />);

    expect(screen.getByRole('status').textContent).toContain('大内容模式');

    const lineNumberDivs = container.querySelectorAll('[data-line-number] .shrink-0.text-right');
    expect(lineNumberDivs.length).toBeLessThan(1000);
    expect(lineNumberDivs[0].textContent?.trim()).toBe('1');
    expect(screen.getByText('1/3')).toBeDefined();
    expect(container.textContent).not.toContain('Line 2505');

    const scrollContainer = container.querySelector('.flex-1.overflow-auto.bg-white') as HTMLDivElement;
    fireEvent.scroll(scrollContainer, { target: { scrollTop: 17_640 } });

    const scrolledLineNumbers = container.querySelectorAll('[data-line-number] .shrink-0.text-right');
    expect(scrolledLineNumbers.length).toBeLessThan(1000);
    expect(scrolledLineNumbers[0].textContent?.trim()).toBe('973');
    expect(container.textContent).toContain('Line 981');
    expect(container.querySelector('[data-line-number="1"]')).toBeNull();

    const nextButton = screen.getAllByRole('button')[1];
    fireEvent.click(nextButton);

    const secondPageLineNumbers = container.querySelectorAll('[data-line-number] .shrink-0.text-right');
    expect(secondPageLineNumbers.length).toBeLessThan(1000);
    expect(secondPageLineNumbers[0].textContent?.trim()).toBe('1001');
    expect(screen.getByText('2/3')).toBeDefined();
  });
});
