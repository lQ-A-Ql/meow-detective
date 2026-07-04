import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listLoadedRulePacks, loadRulePack, validateRulePack } from '@/lib/api/rule-packs';

export function useLoadedRulePacks() {
  return useQuery({
    queryKey: ['rule-packs', 'loaded'],
    queryFn: listLoadedRulePacks,
    retry: false,
  });
}

export function useLoadRulePack() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string) => loadRulePack(path),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['rule-packs', 'loaded'] });
    },
  });
}

export function useValidateRulePack() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (packId: string) => validateRulePack(packId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['rule-packs', 'loaded'] });
    },
  });
}
