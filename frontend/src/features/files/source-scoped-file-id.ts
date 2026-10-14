/** Parse the transport's source-scoped ID without losing local-ID separators. */
export function parseSourceScopedFileId(value?: string) {
  if (!value?.startsWith('ds:') || value.length > 512) return undefined;
  const separator = value.indexOf(':', 3);
  if (separator < 0) return undefined;
  const sourceId = value.slice(3, separator);
  const localId = value.slice(separator + 1);
  if (!/^[a-zA-Z0-9_-]+$/.test(sourceId) || !localId) return undefined;
  return { sourceId, localId };
}

export function isSourceScopedMftId(value?: string) {
  const parsed = parseSourceScopedFileId(value);
  return parsed !== undefined && /^mft:[0-9]+(?::[0-9]+)?$/.test(parsed.localId);
}
