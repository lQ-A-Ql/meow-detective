import { describe, expect, it, vi } from 'vitest';
import { confirmEmulationBoot } from './boot-consent';

describe('confirmEmulationBoot', () => {
  it('requires confirmation before booting the original system without PE media', () => {
    const confirm = vi.fn().mockReturnValue(true);

    expect(confirmEmulationBoot('', 'confirm direct boot', confirm)).toBe(true);
    expect(confirm).toHaveBeenCalledOnce();
    expect(confirm).toHaveBeenCalledWith('confirm direct boot');
  });

  it('cancels direct boot when confirmation is declined', () => {
    expect(confirmEmulationBoot('', 'confirm direct boot', () => false)).toBe(false);
  });

  it('uses explicitly selected PE media without a direct-boot prompt', () => {
    const confirm = vi.fn();

    expect(confirmEmulationBoot('C:\\Tools\\WinPE.iso', 'unused', confirm)).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
  });
});
