import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Search } from './Search';

vi.mock('@/features/search/containers/SearchWorkspaceContainer', () => ({
  SearchWorkspaceContainer: () => <div data-testid="search-workspace" />,
}));

describe('Search page', () => {
  it('renders the search feature container as the page body', () => {
    render(<Search />);
    expect(screen.getByTestId('search-workspace')).toBeInTheDocument();
  });
});
