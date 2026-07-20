import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GqlAutocomplete } from './GqlAutocomplete';
import type { AutocompleteSuggestion } from './gql-language';

const suggestions: AutocompleteSuggestion[] = [
  { label: 'MATCH', insertText: 'MATCH ', description: 'Start a pattern match', kind: 'keyword' },
  { label: 'File', insertText: 'File', description: 'File node type', kind: 'type' },
  { label: 'confidence', insertText: 'confidence', description: 'Edge property', kind: 'property' },
];

describe('GqlAutocomplete', () => {
  it('renders every suggestion label and description', () => {
    render(
      createElement(GqlAutocomplete, {
        suggestions,
        selectedSuggestion: 0,
        applySuggestion: vi.fn(),
        setSelectedSuggestion: vi.fn(),
      }),
    );

    for (const s of suggestions) {
      expect(screen.getByText(s.label)).toBeDefined();
      expect(screen.getByText(s.description)).toBeDefined();
    }
  });

  it('calls applySuggestion with the clicked suggestion', () => {
    const applySuggestion = vi.fn();
    render(
      createElement(GqlAutocomplete, {
        suggestions,
        selectedSuggestion: 0,
        applySuggestion,
        setSelectedSuggestion: vi.fn(),
      }),
    );

    fireEvent.click(screen.getByText('File'));
    expect(applySuggestion).toHaveBeenCalledWith(suggestions[1]);
  });

  it('calls setSelectedSuggestion on hover', () => {
    const setSelectedSuggestion = vi.fn();
    render(
      createElement(GqlAutocomplete, {
        suggestions,
        selectedSuggestion: 0,
        applySuggestion: vi.fn(),
        setSelectedSuggestion,
      }),
    );

    fireEvent.mouseEnter(screen.getByText('confidence'));
    expect(setSelectedSuggestion).toHaveBeenCalledWith(2);
  });

  it('highlights the currently selected suggestion', () => {
    render(
      createElement(GqlAutocomplete, {
        suggestions,
        selectedSuggestion: 1,
        applySuggestion: vi.fn(),
        setSelectedSuggestion: vi.fn(),
      }),
    );

    const highlighted = screen.getByText('File').closest('button');
    expect(highlighted?.className).toContain('bg-forensics-primary-blue');
    const notHighlighted = screen.getByText('MATCH').closest('button');
    expect(notHighlighted?.className).not.toContain('bg-forensics-primary-blue');
  });

  it('renders nothing when there are no suggestions', () => {
    render(
      createElement(GqlAutocomplete, {
        suggestions: [],
        selectedSuggestion: 0,
        applySuggestion: vi.fn(),
        setSelectedSuggestion: vi.fn(),
      }),
    );

    expect(screen.queryByRole('button')).toBeNull();
  });
});
