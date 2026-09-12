import { describe, expect, it, vi } from 'vitest';
import { refreshAnalysisQueries } from './refresh';

describe('refreshAnalysisQueries', () => {
  it('does not issue Windows or Linux analysis queries for Android sources', async () => {
    const windowsQuery = vi.fn(async () => ({}));
    const linuxQuery = vi.fn(async () => ({}));

    await refreshAnalysisQueries('android', [windowsQuery], [linuxQuery]);

    expect(windowsQuery).not.toHaveBeenCalled();
    expect(linuxQuery).not.toHaveBeenCalled();
  });
});
