import { useMutation } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { calculateEvidenceDigest } from '@/lib/api/digest';
import { errorMessage } from '@/lib/errors';
import type { DigestAlgorithm, EvidenceDigest } from '@/types/digest';

interface FileDigestTarget {
  caseId?: string;
  fileId?: string;
  available: boolean;
}

interface DigestOperation {
  key: string;
  epoch: number;
  attempt: number;
  fileId: string;
  algorithm: DigestAlgorithm;
}

/** Keep every mutation terminal state attached to its original selection. */
export function useFileDigest({ caseId, fileId, available }: FileDigestTarget) {
  const key = available && caseId && fileId ? JSON.stringify([caseId, fileId]) : undefined;
  const [selection, setSelection] = useState({ key, epoch: 0 });
  const attempt = useRef(0);
  // Invalidate the previous selection before rendering its children, including A -> B -> A.
  if (selection.key !== key) {
    setSelection({ key, epoch: selection.epoch + 1 });
  }
  const mutation = useMutation<
    { operation: DigestOperation; digest: EvidenceDigest },
    unknown,
    DigestOperation
  >({
    mutationFn: async (operation) => ({
      operation,
      digest: await calculateEvidenceDigest({
        scope: 'file', algorithm: operation.algorithm, fileId: operation.fileId,
      }),
    }),
  });
  const operation = mutation.variables;
  const current = key !== undefined && selection.key === key
    && operation?.key === key && operation.epoch === selection.epoch
    && operation.attempt === attempt.current;
  const result = current && mutation.data?.operation === operation ? mutation.data.digest : undefined;

  return {
    data: result,
    isPending: current && mutation.isPending,
    errorText: current && mutation.error != null ? errorMessage(mutation.error) : undefined,
    available: key !== undefined,
    mutate: (algorithm: DigestAlgorithm) => {
      if (!key || !fileId || selection.key !== key) return;
      mutation.mutate({ key, epoch: selection.epoch, attempt: ++attempt.current, fileId, algorithm });
    },
  };
}
