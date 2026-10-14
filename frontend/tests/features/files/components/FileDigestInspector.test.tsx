import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FileDigestInspector } from '@/features/files/components/FileDigestInspector';

describe('file digest controls', () => {
  it('disables computation without an eligible evidence object', () => {
    const calculate = vi.fn();
    const view = render(<FileDigestInspector available={false} loading={false} onCalculate={calculate} />);
    const button = screen.getByRole('button', { name: 'SHA256' });
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(calculate).not.toHaveBeenCalled();
    view.rerender(<FileDigestInspector available loading={false} onCalculate={calculate} />);
    fireEvent.click(button);
    expect(calculate).toHaveBeenCalledWith('sha256');
  });
});
