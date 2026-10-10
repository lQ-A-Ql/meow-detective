import type { ActionIconName } from '@/assets/icons/actions';

/** One place that decides which drawn hash icon an algorithm uses. */
export function digestIconName(algorithm: string): ActionIconName {
  const normalized = algorithm.toLowerCase().replace(/[-_\s]/g, '');
  if (normalized.startsWith('sha256')) return 'hash-sha256';
  if (normalized.startsWith('sha1')) return 'hash-sha1';
  if (normalized.startsWith('md5')) return 'hash-md5';
  if (normalized.startsWith('sm3')) return 'hash-sm3';
  return 'hash';
}

/** Infers the algorithm from a pasted hash: 32 hex = MD5, 40 = SHA-1, 64 = SHA-256. */
export function digestIconNameForHash(hash: string): ActionIconName {
  const length = hash.trim().length;
  if (length === 32) return 'hash-md5';
  if (length === 40) return 'hash-sha1';
  if (length === 64) return 'hash-sha256';
  return 'hash';
}
