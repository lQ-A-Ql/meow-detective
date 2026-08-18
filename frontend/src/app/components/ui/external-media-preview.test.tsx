import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ExternalMediaPreview } from './external-media-preview';

const labels = {
  warningLabel: 'Unverified external candidate',
  unavailableLabel: 'Candidate unavailable',
  blockedLabel: 'Source blocked',
};

describe('ExternalMediaPreview', () => {
  it('renders an allowed qpic image with an evidence-boundary warning', () => {
    render(
      <ExternalMediaPreview
        {...labels}
        sourceUrl="http://mmsns.qpic.cn/mmsns/example/0"
        alt="Moment image"
      />,
    );

    expect(screen.getByText(labels.warningLabel)).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Moment image' })).toHaveAttribute(
      'src',
      'http://mmsns.qpic.cn/mmsns/example/0',
    );
    expect(screen.getByRole('img', { name: 'Moment image' })).toHaveAttribute(
      'referrerpolicy',
      'no-referrer',
    );
  });

  it('does not create a network image for a non-qpic URL', () => {
    render(
      <ExternalMediaPreview
        {...labels}
        sourceUrl="https://media.invalid/image.jpg"
        alt="Moment image"
      />,
    );

    expect(screen.getByText(labels.blockedLabel)).toBeInTheDocument();
    expect(document.querySelector('img[src="https://media.invalid/image.jpg"]')).toBeNull();
  });
});
