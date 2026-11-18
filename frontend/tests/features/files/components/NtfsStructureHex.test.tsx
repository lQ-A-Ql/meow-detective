import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { NtfsStructureHex } from '@/features/files/components/NtfsStructureHex';

describe('NtfsStructureHex', () => {
  it('offers an icon-only toggle whose accessible name follows the decoded state', () => {
    const onDecodedChange = vi.fn();
    const { rerender } = render(
      <NtfsStructureHex
        hex={{ bytes: [0x46, 0x49, 0x4c, 0x45], offset: 0, length: 4, decoded: false }}
        sourceRanges={[]}
        onDecodedChange={onDecodedChange}
      />,
    );

    const toggle = screen.getByRole('button', { name: '显示解码字节' });
    expect(toggle).toHaveAttribute('data-shape', 'circle');
    expect(toggle.textContent).toBe('');
    expect(toggle.querySelector('svg')).not.toBeNull();
    toggle.click();
    expect(onDecodedChange).toHaveBeenCalledWith(true);

    rerender(
      <NtfsStructureHex
        hex={{ bytes: [0x46, 0x49, 0x4c, 0x45], offset: 0, length: 4, decoded: true }}
        sourceRanges={[]}
        onDecodedChange={onDecodedChange}
      />,
    );

    expect(screen.getByRole('button', { name: '显示原始来源字节' })).toBeInTheDocument();
  });
});
