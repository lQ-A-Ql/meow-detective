import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Emulation } from './Emulation';

vi.mock('@/features/emulation/containers/EmulationWorkspaceContainer', () => ({
  EmulationWorkspaceContainer: () => <div data-testid="emulation-workspace" />,
}));

describe('Emulation page', () => {
  it('renders only the feature container as the page body', () => {
    render(<Emulation />);
    expect(screen.getByTestId('emulation-workspace')).toBeInTheDocument();
  });
});
