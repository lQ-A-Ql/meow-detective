export interface SavedSearchQuery {
  id: string;
  name: string;
  query: string;
  createdAt: string;
}

const STORAGE_KEY = 'forensics.savedSearchQueries';

export function readSavedSearchQueries(): SavedSearchQuery[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isSavedSearchQuery);
  } catch {
    return [];
  }
}

export function writeSavedSearchQueries(queries: SavedSearchQuery[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(queries));
}

export function upsertSavedSearchQuery(
  queries: SavedSearchQuery[],
  name: string,
  query: string,
): SavedSearchQuery[] {
  const trimmedName = name.trim();
  const trimmedQuery = query.trim();
  if (!trimmedName || !trimmedQuery) {
    return queries;
  }
  const now = new Date().toISOString();
  const existing = queries.find((item) => item.name === trimmedName);
  if (existing) {
    return queries.map((item) =>
      item.id === existing.id ? { ...item, query: trimmedQuery, createdAt: now } : item,
    );
  }
  return [
    {
      id: createId(),
      name: trimmedName,
      query: trimmedQuery,
      createdAt: now,
    },
    ...queries,
  ].slice(0, 20);
}

function createId() {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `saved-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function removeSavedSearchQuery(
  queries: SavedSearchQuery[],
  id: string,
): SavedSearchQuery[] {
  return queries.filter((item) => item.id !== id);
}

function isSavedSearchQuery(value: unknown): value is SavedSearchQuery {
  if (!value || typeof value !== 'object') return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.id === 'string' &&
    typeof item.name === 'string' &&
    typeof item.query === 'string' &&
    typeof item.createdAt === 'string'
  );
}
