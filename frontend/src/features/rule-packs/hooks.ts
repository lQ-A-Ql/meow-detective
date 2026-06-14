import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listLoadedRulePacks, loadRulePack, validateRulePack } from '@/lib/api/rule-packs';

export function useLoadedRulePacks() {
  return useQuery({
    queryKey: ['rule-packs', 'loaded'],
    queryFn: listLoadedRulePacks,
  });
}

export function useLoadRulePack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (path: string) => loadRulePack(path),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['rule-packs'] });
    },
  });
}

export function useValidateRulePack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (packId: string) => validateRulePack(packId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['rule-packs'] });
    },
  });
}
