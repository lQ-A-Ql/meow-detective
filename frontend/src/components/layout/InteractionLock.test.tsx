import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InteractionLock } from './InteractionLock';

describe('InteractionLock', () => {
  it('uses inert while locked and removes it after release', () => {
    const { container, rerender } = render(
      <InteractionLock locked>
        <span>workspace</span>
      </InteractionLock>,
    );

    const region = container.firstElementChild;
    expect(region).toHaveAttribute('inert');
    expect(region).toHaveAttribute('aria-busy', 'true');

    rerender(
      <InteractionLock locked={false}>
        <span>workspace</span>
      </InteractionLock>,
    );

    expect(region).not.toHaveAttribute('inert');
    expect(region).toHaveAttribute('aria-busy', 'false');
  });
});
