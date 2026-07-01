import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GqlQueryInput } from './GqlQueryInput';

function typeQuery(textarea: HTMLTextAreaElement, value: string, cursorPos = value.length) {
  fireEvent.input(textarea, { target: { value } });
  textarea.setSelectionRange(cursorPos, cursorPos);
  fireEvent.select(textarea);
}

describe('GqlQueryInput', () => {
  it('renders header, Run button, and textarea', () => {
    render(createElement(GqlQueryInput, {}));
    expect(screen.getByText('GQL Query')).toBeDefined();
    expect(screen.getByRole('button', { name: /Run/ })).toBeDefined();
    expect(screen.getByRole('textbox')).toBeDefined();
  });

  it('displays an error when error is provided', () => {
    render(createElement(GqlQueryInput, { error: 'Syntax error at line 2' }));
    expect(screen.getByText('Syntax error at line 2')).toBeDefined();
  });

  it('calls onExecute with the query when Run is clicked', () => {
    const onExecute = vi.fn();
    render(createElement(GqlQueryInput, { onExecute, initialQuery: 'MATCH (n) RETURN n' }));
    fireEvent.click(screen.getByRole('button', { name: /Run/ }));
    expect(onExecute).toHaveBeenCalledWith('MATCH (n) RETURN n');
  });

  it('calls onExecute when Ctrl+Enter is pressed in the textarea', () => {
    const onExecute = vi.fn();
    render(createElement(GqlQueryInput, { onExecute, initialQuery: 'MATCH (n) RETURN n' }));
    const textarea = screen.getByRole('textbox');
    fireEvent.keyDown(textarea, { key: 'Enter', ctrlKey: true });
    expect(onExecute).toHaveBeenCalledWith('MATCH (n) RETURN n');
  });

  it('shows autocomplete suggestions while typing a partial keyword', () => {
    render(createElement(GqlQueryInput, {}));
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    typeQuery(textarea, 'MATC');
    expect(screen.getByText('MATCH')).toBeDefined();
  });

  it('navigates suggestions with arrow keys and applies with Enter', () => {
    const onQueryChange = vi.fn();
    render(createElement(GqlQueryInput, { onQueryChange }));
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    typeQuery(textarea, 'MATC');

    fireEvent.keyDown(textarea, { key: 'ArrowDown' });
    fireEvent.keyDown(textarea, { key: 'Enter' });

    expect(onQueryChange).toHaveBeenLastCalledWith(expect.stringContaining('MATCH'));
  });

  it('closes the suggestion list on Escape', () => {
    render(createElement(GqlQueryInput, {}));
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    typeQuery(textarea, 'MATC');
    expect(screen.getByText('MATCH')).toBeDefined();

    fireEvent.keyDown(textarea, { key: 'Escape' });
    expect(screen.queryByText('MATCH')).toBeNull();
  });

  it('applies a suggestion when clicked from the dropdown', () => {
    const onQueryChange = vi.fn();
    render(createElement(GqlQueryInput, { onQueryChange }));
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    typeQuery(textarea, 'MATC');

    fireEvent.click(screen.getByText('MATCH'));
    expect(onQueryChange).toHaveBeenLastCalledWith(expect.stringContaining('MATCH'));
  });
});
