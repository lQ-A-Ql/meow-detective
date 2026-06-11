import { describe, it, expect } from 'vitest';
import { searchFiles } from '@/lib/api/search';

describe('search API (mock mode)', () => {
  it('searchFiles returns result page with hits', async () => {
    const result = await searchFiles('test query');
    expect(result).toBeDefined();
    expect(result.total).toBeGreaterThanOrEqual(0);
    expect(Array.isArray(result.items)).toBe(true);
  });

  it('searchFiles result items have required fields', async () => {
    const result = await searchFiles('query');
    if (result.items.length > 0) {
      const hit = result.items[0];
      expect(hit.fileId).toBeDefined();
      expect(hit.path).toBeDefined();
      expect(typeof hit.score).toBe('number');
      expect(Array.isArray(hit.snippets)).toBe(true);
    }
  });

  it('searchFiles accepts offset and limit parameters', async () => {
    const result = await searchFiles('query', 0, 10);
    expect(result).toBeDefined();
    expect(Array.isArray(result.items)).toBe(true);
  });

  it('searchFiles returns tookMs as number', async () => {
    const result = await searchFiles('query');
    expect(typeof result.tookMs).toBe('number');
  });

  it('searchFiles result snippets have text field', async () => {
    const result = await searchFiles('query');
    for (const hit of result.items) {
      for (const snippet of hit.snippets) {
        expect(typeof snippet.text).toBe('string');
      }
    }
  });
});
