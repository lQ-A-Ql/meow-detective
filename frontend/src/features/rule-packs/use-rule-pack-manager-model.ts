import { useLoadedRulePacks, useLoadRulePack, useValidateRulePack } from '@/features/rule-packs/hooks';

const KNOWN_RULE_FAMILIES = [
  'Prefetch',
  'LNK',
  'JumpList',
  'Registry',
  'EventLog',
  'BrowserHistory',
  'UserAssist',
  'RecycleBin',
  'Thumbcache',
  'SRU',
  'Amcache',
  'BAM',
  'MFT',
  'FileSystem',
  'NetworkArtifacts',
];

export function useRulePackManagerModel() {
  const packsQuery = useLoadedRulePacks();
  const loadMutation = useLoadRulePack();
  const validateMutation = useValidateRulePack();
  const packs = packsQuery.data ?? [];
  const coveredSet = new Set(packs.flatMap((pack) => pack.coveredFamilies));
  const coveredFamilies = Array.from(coveredSet);
  const uncoveredFamilies = KNOWN_RULE_FAMILIES.filter((family) => !coveredSet.has(family));

  return {
    packs,
    loading: packsQuery.isLoading,
    error: packsQuery.isError,
    totalRules: packs.reduce((total, pack) => total + pack.ruleCount, 0),
    coveredFamilies,
    uncoveredFamilies,
    coveragePercent: KNOWN_RULE_FAMILIES.length
      ? Math.round((coveredFamilies.length / KNOWN_RULE_FAMILIES.length) * 100)
      : 0,
    loadPending: loadMutation.isPending,
    loadError: mutationErrorMessage(loadMutation.error),
    validatePending: validateMutation.isPending,
    validatingPackId: validateMutation.variables,
    retry() {
      void packsQuery.refetch();
    },
    load(path: string, onSuccess: () => void) {
      loadMutation.mutate(path, { onSuccess });
    },
    validate(packId: string) {
      validateMutation.mutate(packId);
    },
  };
}

function mutationErrorMessage(error: unknown): string | undefined {
  if (!error) return undefined;
  return error instanceof Error ? error.message : '加载失败';
}

export type RulePackManagerModel = ReturnType<typeof useRulePackManagerModel>;
