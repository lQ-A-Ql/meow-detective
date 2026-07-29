import { useCallback, useMemo } from 'react';
import { useCorrelationSnapshot, useV2GovernanceSnapshot } from '@/features/analysis/hooks';
import { useCurrentCase } from '@/features/case/hooks';

/** Owns V2 governance queries and refresh orchestration. */
export function useV2WorkbenchModel() {
  const currentCase = useCurrentCase();
  const snapshot = useV2GovernanceSnapshot();
  const correlation = useCorrelationSnapshot();
  const refresh = useCallback(async () => {
    await Promise.all([snapshot.refetch(), correlation.refetch()]);
  }, [correlation, snapshot]);
  const error = useMemo(
    () => currentCase.error ?? snapshot.error ?? correlation.error,
    [correlation.error, currentCase.error, snapshot.error],
  );

  return {
    correlation,
    error,
    hasCase: Boolean(currentCase.data),
    loading: currentCase.isLoading,
    refresh,
    snapshot,
    currentCaseIsSuccess: currentCase.isSuccess,
  };
}

export type V2WorkbenchModel = ReturnType<typeof useV2WorkbenchModel>;
