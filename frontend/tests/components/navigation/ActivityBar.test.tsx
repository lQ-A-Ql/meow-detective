import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router';
import i18n from '@/i18n';
import { ActivityBar } from '@/components/navigation/ActivityBar';
import type { NavigationModel } from '@/features/shell/use-navigation-model';

const model: NavigationModel = {
  links: [
    { to: '/search', page: 'search' },
    { to: '/timeline', page: 'timeline' },
  ],
  runningCount: 2,
  toggleDrawer: vi.fn(),
  openSettings: vi.fn(),
};

function renderBar() {
  return render(
    <MemoryRouter>
      <ActivityBar model={model} />
    </MemoryRouter>,
  );
}

describe('ActivityBar', () => {
  it('labels each rail entry from the navigation copy and shows it through the shared tooltip', async () => {
    renderBar();
    const label = i18n.t('navigation.links.search.label');
    const link = screen.getByRole('link', { name: label });

    link.focus();

    expect(await screen.findByRole('tooltip')).toHaveTextContent(label);
  });

  it('routes the job badge and settings entry through the same tooltip primitive', async () => {
    renderBar();
    const settings = i18n.t('settings.title');

    screen.getByRole('button', { name: settings }).focus();

    expect(await screen.findByRole('tooltip')).toHaveTextContent(settings);
    expect(screen.getByRole('button', { name: i18n.t('navigation.jobsRunning', { count: 2 }) })).toBeInTheDocument();
  });

  it('navigates through the router and marks the active rail entry from the location', async () => {
    renderBar();
    const label = i18n.t('navigation.links.timeline.label');

    await act(async () => {
      screen.getByRole('link', { name: label }).click();
    });

    expect(screen.getByRole('link', { name: label })).toHaveAttribute('aria-current', 'page');
  });
});
