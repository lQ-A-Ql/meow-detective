import { beforeEach, describe, expect, it } from 'vitest';
import { useDensityStore } from '@/stores/density-store';

describe('density store', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.dataset.density = 'compact';
  });

  it('toggles the document attribute and persists the preference', () => {
    useDensityStore.getState().setDensity('compact');
    expect(document.documentElement.dataset.density).toBe('compact');

    useDensityStore.getState().toggleDensity();
    expect(useDensityStore.getState().density).toBe('comfortable');
    expect(document.documentElement.dataset.density).toBe('comfortable');
    expect(localStorage.getItem('meowDetective.density')).toBe('comfortable');

    useDensityStore.getState().toggleDensity();
    expect(document.documentElement.dataset.density).toBe('compact');
  });
});
