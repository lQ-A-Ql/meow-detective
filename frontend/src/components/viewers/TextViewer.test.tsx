import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { TextViewer } from './TextViewer';

describe('TextViewer', () => {
  it('renders text lines', () => {
    const content = ['Hello World', 'Second line', 'Third line'].join('\n');
    render(<TextViewer content={content} encoding="UTF-8" />);

    expect(screen.getByText('Hello World')).toBeDefined();
    expect(screen.getByText('Second line')).toBeDefined();
    expect(screen.getByText('Third line')).toBeDefined();
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
});
