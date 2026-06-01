import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  readSavedSearchQueries,
  removeSavedSearchQuery,
  upsertSavedSearchQuery,
  writeSavedSearchQueries,
} from './saved-queries';

describe('saved search queries', () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => store.set(key, value),
      removeItem: (key: string) => store.delete(key),
      clear: () => store.clear(),
    });
  });

  it('persists and reads saved queries from localStorage', () => {
    const queries = upsertSavedSearchQuery([], 'Documents', 'extension:doc');

    writeSavedSearchQueries(queries);

    expect(readSavedSearchQueries()).toEqual(queries);
  });

  it('updates an existing query by name', () => {
    const first = upsertSavedSearchQuery([], 'Interesting files', 'exe');
    const second = upsertSavedSearchQuery(first, 'Interesting files', 'dll');

    expect(second).toHaveLength(1);
    expect(second[0].id).toBe(first[0].id);
    expect(second[0].query).toBe('dll');
  });

  it('removes a saved query by id', () => {
    const queries = upsertSavedSearchQuery([], 'A', 'one');

    expect(removeSavedSearchQuery(queries, queries[0].id)).toEqual([]);
  });
});
