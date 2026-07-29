import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ViewerTabFrame } from './ViewerTabFrame';

describe('ViewerTabFrame', () => {
  it('leaves scrolling to the active viewer content', () => {
    const { container } = render(
      <ViewerTabFrame
        value="hex"
        onValueChange={vi.fn()}
        tabs={[
          {
            value: 'hex',
            label: 'Hex',
            content: <div>hex content</div>,
            contentClassName: 'p-0',
          },
          {
            value: 'text',
            label: 'Text',
            content: <div>text content</div>,
          },
        ]}
      />,
    );

    expect(screen.getByText('hex content')).toBeInTheDocument();
    expect(container.querySelector('[data-slot="scroll-area"]')).toBeNull();
  });
});
