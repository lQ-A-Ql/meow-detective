import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TextViewer } from '@/components/viewers/TextViewer';
import { Dialog, DialogContent, DialogTitle } from '@/app/components/ui/dialog';

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
    expect(screen.getByText('第 1/3 页')).toBeDefined();
    expect(screen.queryByText('Line 2505')).toBeNull();

    const scrollContainer = screen.getByTestId('text-scroll-container');
    fireEvent.scroll(scrollContainer, { target: { scrollTop: 17_640 } });

    const scrolledLineNumbers = screen.getAllByTestId('text-line-number');
    expect(scrolledLineNumbers.length).toBeLessThan(1000);
    expect(scrolledLineNumbers[0].textContent?.trim()).toBe('973');
    expect(screen.getByText('Line 981')).toBeDefined();
    expect(scrolledLineNumbers[0].getAttribute('data-line-number')).not.toBe('1');

    const nextButton = screen.getByRole('button', { name: '下一页' });
    fireEvent.click(nextButton);

    const secondPageLineNumbers = screen.getAllByTestId('text-line-number');
    expect(secondPageLineNumbers.length).toBeLessThan(1000);
    expect(secondPageLineNumbers[0].textContent?.trim()).toBe('1001');
    expect(screen.getByText('第 2/3 页')).toBeDefined();
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

  it('finds a tail occurrence on another page and preserves the reading position when cleared', async () => {
    const lines = Array.from({ length: 1376 }, (_, i) => `config item ${i + 1}`);
    lines[1374] = 'requirepass synthetic-value';
    render(<TextViewer content={lines.join('\n')} encoding="UTF-8" />);
    const input = screen.getByRole('textbox', { name: '搜索文本' });
    fireEvent.change(input, { target: { value: 'pass' } });
    await waitFor(() => expect(document.querySelector('mark[data-current="true"]')).toHaveTextContent('pass'));
    expect(screen.getByText('第 2/2 页')).toBeInTheDocument();
    expect(screen.getByRole('status', { name: '搜索命中' })).toHaveTextContent('命中 1/1');
    expect(document.querySelector('[data-line-number="1375"]')).toHaveAttribute('data-active', 'true');
    const viewport = screen.getByTestId('text-scroll-container');
    expect(viewport.scrollTop).toBeGreaterThan(6000);
    const previousScroll = viewport.scrollTop;
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(input).toHaveValue('');
    expect(viewport.scrollTop).toBe(previousScroll);
    expect(screen.getByText('第 2/2 页')).toBeInTheDocument();
    expect(document.querySelector('mark')).toBeNull();
  });

  it('moves between occurrences, including multiple matches in one line, with keyboard shortcuts', () => {
    render(<TextViewer content={'pass and PASS\nlast pass'} encoding="UTF-8" />);
    const input = screen.getByRole('textbox', { name: '搜索文本' });
    fireEvent.change(input, { target: { value: 'pass' } });
    const current = () => document.querySelector('mark[data-current="true"]');
    expect(current()).toHaveTextContent('pass');
    expect(current()).toHaveAttribute('data-match-index', '0');
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(current()).toHaveTextContent('PASS');
    expect(current()).toHaveAttribute('data-match-index', '1');
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
    expect(current()).toHaveAttribute('data-match-index', '0');
    fireEvent.click(screen.getByRole('button', { name: '上一处命中' }));
    expect(current()).toHaveAttribute('data-match-index', '2');
    expect(screen.getByRole('status', { name: '搜索命中' })).toHaveTextContent('命中 3/3');
  });

  it('locates both sides of a 1000-line page boundary', () => {
    const lines = Array.from({ length: 1002 }, () => 'ordinary');
    lines[999] = 'needle before';
    lines[1000] = 'needle after';
    render(<TextViewer content={lines.join('\n')} encoding="UTF-8" searchQuery="needle" />);
    expect(document.querySelector('[data-line-number="1000"]')).toHaveAttribute('data-active', 'true');
    fireEvent.click(screen.getByRole('button', { name: '下一处命中' }));
    expect(screen.getByText('第 2/2 页')).toBeInTheDocument();
    expect(document.querySelector('[data-line-number="1001"]')).toHaveAttribute('data-active', 'true');
    fireEvent.click(screen.getByRole('button', { name: '上一处命中' }));
    expect(document.querySelector('[data-line-number="1000"]')).toHaveAttribute('data-active', 'true');
  });

  it('highlights one occurrence crossing a long-line segment boundary without inserting spaces', () => {
    render(<TextViewer content={'x'.repeat(8190) + 'needle' + 'end'} encoding="UTF-8" searchQuery="needle" />);
    expect(screen.getByRole('status', { name: '搜索命中' })).toHaveTextContent('命中 1/1');
    const fragments = Array.from(document.querySelectorAll('mark[data-current="true"]'));
    expect(fragments).toHaveLength(1);
    expect(fragments.map((mark) => mark.textContent).join('')).toBe('needle');
    expect(screen.getAllByTestId('text-line-content').map((line) => line.textContent).join('')).toBe('x'.repeat(8190) + 'needle' + 'end');
  });

  it('resets search on document changes and allows editing an externally supplied query', () => {
    const { rerender } = render(<TextViewer documentKey="one" content="pass needle" encoding="UTF-8" searchQuery="pass" />);
    const input = screen.getByRole('textbox', { name: '搜索文本' });
    fireEvent.change(input, { target: { value: 'needle' } });
    expect(document.querySelector('mark[data-current="true"]')).toHaveTextContent('needle');
    rerender(<TextViewer documentKey="two" content="pass needle" encoding="UTF-8" />);
    expect(input).toHaveValue('');
    expect(document.querySelector('mark')).toBeNull();
    rerender(<TextViewer documentKey="two" content="pass needle" encoding="UTF-8" searchQuery="pass" />);
    expect(input).toHaveValue('pass');
    expect(document.querySelector('mark[data-current="true"]')).toHaveTextContent('pass');
  });

  it('reports no matches and makes a truncated preview search scope explicit', () => {
    render(<TextViewer content="ordinary text" encoding="UTF-8" isTruncated />);
    fireEvent.change(screen.getByRole('textbox', { name: '搜索文本' }), { target: { value: 'missing' } });
    expect(screen.getByText('未找到匹配')).toBeInTheDocument();
    expect(screen.getByText(/搜索范围：当前已载入预览/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一处命中' })).toBeDisabled();
  });

  it('clears an active search before Escape closes a containing preview dialog', () => {
    render(<Dialog defaultOpen><DialogContent aria-describedby={undefined}>
      <DialogTitle>Mock preview</DialogTitle><TextViewer content="pass sample" encoding="UTF-8" />
    </DialogContent></Dialog>);
    const input = screen.getByRole('textbox', { name: '搜索文本' });
    fireEvent.change(input, { target: { value: 'pass' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(input).toHaveValue('');
    expect(screen.getByRole('dialog', { name: 'Mock preview' })).toBeInTheDocument();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
