import { render, screen } from '@testing-library/react';
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
  selectPage: vi.fn(),
  toggleDrawer: vi.fn(),
  openSettings: vi.fn(),
};

function renderBar() {
  return render(
    <MemoryRouter>
      <ActivityBar model={model} density="compact" onToggleDensity={vi.fn()} />
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

  it('marks the current page in the store when a rail entry is used', async () => {
    renderBar();

    screen.getByRole('link', { name: i18n.t('navigation.links.timeline.label') }).click();

    expect(model.selectPage).toHaveBeenCalledWith('timeline');
  });
});
