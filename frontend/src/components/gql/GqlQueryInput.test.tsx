import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GqlQueryInput } from './GqlQueryInput';

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
});
