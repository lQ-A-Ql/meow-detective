import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ShellContainer } from '@/features/shell/containers/ShellContainer';

const state = vi.hoisted(() => ({ current: null as { id: string } | null, toggleDrawer: vi.fn() }));
vi.mock('@/features/case/hooks', () => ({ useCurrentCase: () => ({ data: state.current }) }));
vi.mock('@/features/shell/use-navigation-model', () => ({ useNavigationModel: () => ({
  links: [{ to: '/', page: 'home' }, { to: '/files', page: 'files' }], selectPage: vi.fn(), toggleDrawer: state.toggleDrawer,
}) }));
vi.mock('@/features/shell/containers/ActivityBarContainer', () => ({ ActivityBarContainer: () => <nav aria-label="workspace navigation" /> }));
vi.mock('@/features/jobs/containers/BottomDrawerContainer', () => ({ BottomDrawerContainer: () => <div role="region" aria-label="job drawer" /> }));

function shell(path = '/') {
  return <MemoryRouter initialEntries={[path]}><ShellContainer><span>page</span></ShellContainer></MemoryRouter>;
}

describe('case entry chrome', () => {
  beforeEach(() => { state.current = null; state.toggleDrawer.mockClear(); });
  it('hides navigation and jobs only for the empty root entry and restores them when a case opens', () => {
    const result = render(shell());
    expect(screen.queryByRole('navigation')).toBeNull();
    expect(screen.queryByRole('region', { name: 'job drawer' })).toBeNull();
    state.current = { id: 'case-1' };
    result.rerender(shell());
    expect(screen.getByRole('navigation')).toBeDefined();
    expect(screen.getByRole('region', { name: 'job drawer' })).toBeDefined();
    state.current = null;
    result.rerender(shell());
    expect(screen.queryByRole('navigation')).toBeNull();
  });
  it('keeps chrome on workspace routes even without a case', () => {
    render(shell('/files'));
    expect(screen.getByRole('navigation')).toBeDefined();
    expect(screen.getByRole('region', { name: 'job drawer' })).toBeDefined();
  });
  it('removes the unavailable drawer command from the empty entry', () => {
    render(shell());
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    expect(screen.queryByRole('option', { name: /JOBS/ })).toBeNull();
  });
});
