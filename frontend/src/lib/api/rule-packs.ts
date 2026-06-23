import { RulePackSummary, RulePackValidationResult } from '@/types/models';
import { apiClient } from './client';

export async function listLoadedRulePacks(): Promise<RulePackSummary[]> {
  return apiClient.request('list_loaded_rule_packs');
}

export async function loadRulePack(path: string): Promise<RulePackSummary> {
  return apiClient.request('load_rule_pack', { request: { path } });
}

export async function validateRulePack(packId: string): Promise<RulePackValidationResult> {
  return apiClient.request('validate_rule_pack', { request: { packId } });
}
