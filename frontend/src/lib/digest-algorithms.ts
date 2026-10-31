import type { DigestAlgorithm } from '@/types/digest';

export const DIGEST_ALGORITHMS: readonly { value: DigestAlgorithm; label: string }[] = [
  { value: 'md5', label: 'MD5' },
  { value: 'sha1', label: 'SHA-1' },
  { value: 'sha256', label: 'SHA-256' },
  { value: 'sm3', label: 'SM3' },
];
