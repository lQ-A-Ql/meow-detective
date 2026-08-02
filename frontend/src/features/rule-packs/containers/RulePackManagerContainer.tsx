import { RulePackManager } from '@/features/rule-packs/components/RulePackManager';
import { useRulePackManagerModel } from '@/features/rule-packs/use-rule-pack-manager-model';

export function RulePackManagerContainer() {
  const model = useRulePackManagerModel();
  return <RulePackManager model={model} />;
}
