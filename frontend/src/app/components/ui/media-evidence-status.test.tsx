import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MediaEvidenceStatus } from './media-evidence-status';

describe('MediaEvidenceStatus', () => {
  it('labels an encrypted local cache without presenting it as an image', () => {
    render(
      <MediaEvidenceStatus
        status="linked-encrypted"
        label="Local encrypted cache linked; image key unavailable"
        detail="83d35dbfebf20beff6c1e711168205ee"
      />,
    );

    expect(screen.getByText('Local encrypted cache linked; image key unavailable')).toBeInTheDocument();
    expect(screen.getByText('83d35dbfebf20beff6c1e711168205ee')).toBeInTheDocument();
    expect(document.querySelector('[data-media-evidence-status="linked-encrypted"]')).toBeInTheDocument();
  });
});
