import type { ActionIconName } from '@/components/controls/ActionIcon';

/** Infers the algorithm from a pasted hash: 32 hex = MD5, 40 = SHA-1, 64 = SHA-256. */
export function digestIconNameForHash(hash: string): ActionIconName {
  const length = hash.trim().length;
  if (length === 32) return 'hash-md5';
  if (length === 40) return 'hash-sha1';
  if (length === 64) return 'hash-sha256';
  return 'hash';
}
