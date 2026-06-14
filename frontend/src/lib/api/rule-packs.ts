import { RulePackSummary, RulePackValidationResult } from '@/types/models';
import { apiClient } from './client';

export async function listLoadedRulePacks(): Promise<RulePackSummary[]> {
  return apiClient.request(
    'list_loaded_rule_packs',
    () => apiClient.getMockProvider().listLoadedRulePacks(),
  );
}

export async function loadRulePack(path: string): Promise<RulePackSummary> {
  return apiClient.request(
    'load_rule_pack',
    () => apiClient.getMockProvider().loadRulePack(path),
    { request: { path } },
  );
}

export async function validateRulePack(packId: string): Promise<RulePackValidationResult> {
  return apiClient.request(
    'validate_rule_pack',
    () => apiClient.getMockProvider().validateRulePack(packId),
    { request: { packId } },
  );
}
