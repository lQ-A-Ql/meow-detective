import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listPluginActions, recoverWeChatKeys } from '@/lib/api/analysis';
import { useCurrentCase } from '@/features/case/hooks';

/**
 * Plugin action channel hooks (plugin ABI optional fourth export).
 * `usePluginActions` lists a plugin's self-described actions;
 * `useRecoverWeChatKeys` runs the host-side key-recovery command and
 * refreshes plugin module/family caches so counts reflect the rerun.
 */
export function usePluginActions(pluginId?: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'plugin-actions', currentCase.data?.id ?? null, pluginId ?? null],
    queryFn: () => listPluginActions(pluginId ?? ''),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(pluginId),
    retry: false,
    staleTime: Infinity,
    gcTime: Infinity,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
}

export function useRecoverWeChatKeys() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  return useMutation({
    mutationFn: (request: { dataSourceId: string; dumpPath: string }) =>
      recoverWeChatKeys(request.dataSourceId, request.dumpPath),
    onSuccess: async (_data, variables) => {
      const caseId = currentCase.data?.id ?? null;
      await Promise.all([
        qc.invalidateQueries({
          queryKey: ['analysis', 'plugin-modules', caseId, variables.dataSourceId],
          refetchType: 'active',
        }),
        qc.invalidateQueries({
          queryKey: ['analysis', 'plugin-family-entries', caseId, variables.dataSourceId],
          refetchType: 'active',
        }),
      ]);
    },
  });
}
