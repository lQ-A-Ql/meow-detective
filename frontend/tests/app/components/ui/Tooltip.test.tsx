import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Tooltip } from '@/app/components/ui/tooltip';

describe('Tooltip', () => {
  it('exposes its content as an accessible tooltip when the trigger receives focus', async () => {
    render(
      <Tooltip content="Shared hint">
        <button type="button">trigger</button>
      </Tooltip>,
    );

    const trigger = screen.getByRole('button', { name: 'trigger' });
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();

    trigger.focus();

    const tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Shared hint');
    expect(trigger).toHaveAttribute('aria-describedby', tooltip.id);
  });
});
