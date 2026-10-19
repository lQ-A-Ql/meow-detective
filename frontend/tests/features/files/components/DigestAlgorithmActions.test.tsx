import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DigestAlgorithmActions } from '@/features/files/components/DigestAlgorithmActions';

describe('file digest controls', () => {
  it('disables computation without an eligible evidence object', () => {
    const calculate = vi.fn();
    const view = render(<DigestAlgorithmActions available={false} loading={false} onCalculate={calculate} />);
    const button = screen.getByRole('button', { name: '计算 SHA-256' });
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(calculate).not.toHaveBeenCalled();
    view.rerender(<DigestAlgorithmActions available loading={false} onCalculate={calculate} />);
    fireEvent.click(button);
    expect(calculate).toHaveBeenCalledWith('sha256');
  });
  it('places each algorithm label after its icon button and locks every action while running', () => {
    render(<DigestAlgorithmActions available loading onCalculate={vi.fn()} />);
    for (const label of ['MD5', 'SHA-1', 'SHA-256', 'SM3']) {
      const button = screen.getByRole('button', { name: `计算 ${label}` });
      expect(button).toBeDisabled();
      expect(button).toHaveTextContent('');
      expect(button.nextElementSibling).toHaveTextContent(label);
    }
  });
});
