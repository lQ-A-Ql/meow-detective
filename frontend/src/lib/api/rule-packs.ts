import { RulePackSummary, RulePackValidationResult } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function listLoadedRulePacks(): Promise<RulePackSummary[]> {
  return apiClient.request(COMMANDS.rulePacks.LIST_LOADED_RULE_PACKS);
}

export async function loadRulePack(path: string): Promise<RulePackSummary> {
  return apiClient.request(COMMANDS.rulePacks.LOAD_RULE_PACK, { request: { path } });
}

export async function validateRulePack(packId: string): Promise<RulePackValidationResult> {
  return apiClient.request(COMMANDS.rulePacks.VALIDATE_RULE_PACK, { request: { packId } });
}
