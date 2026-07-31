import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DenseDataTableFrame } from './DenseDataTableFrame';

describe('DenseDataTableFrame', () => {
  it('sizes an embedded table from its actual rows', () => {
    const { container } = render(
      <DenseDataTableFrame rowCount={2}>
        <div>table</div>
      </DenseDataTableFrame>,
    );

    expect(container.firstElementChild).toHaveStyle({
      height: 'min(92px, min(60vh, 45rem))',
    });
  });

  it('fills an already bounded viewport without adding a second height', () => {
    const { container } = render(
      <DenseDataTableFrame layout="fill">
        <div>table</div>
      </DenseDataTableFrame>,
    );

    expect(container.firstElementChild?.className).toContain('flex-1');
    expect(container.firstElementChild?.className).not.toContain('h-full');
    expect(container.firstElementChild).not.toHaveAttribute('style');
  });

  it('includes a frame header and supports the compact maximum', () => {
    const { container } = render(
      <DenseDataTableFrame
        rowCount={2}
        header={<div>group</div>}
        maxHeight="compact"
      >
        <div>table</div>
      </DenseDataTableFrame>,
    );

    expect(container.firstElementChild).toHaveStyle({
      height: 'min(126px, min(60vh, 35rem))',
    });
  });

  it('reserves readable space for an empty-table message', () => {
    const { container } = render(
      <DenseDataTableFrame rowCount={0}>
        <div>empty table</div>
      </DenseDataTableFrame>,
    );

    expect(container.firstElementChild).toHaveStyle({
      height: 'min(128px, min(60vh, 45rem))',
    });
  });
});
