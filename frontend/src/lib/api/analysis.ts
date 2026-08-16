import {
  AnalysisExtractionPageRequest,
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  CaseOverviewSnapshot,
  CorrelationSnapshot,
  EmailExtractionSummary,
  EvtxEventSummary,
  EvtxEventPageRequest,
  EvidenceClassificationSummary,
  FileClassificationBoard,
  LinuxArtifactSummary,
  PluginActionDescriptor,
  PluginFamilyEntries,
  PluginFamilyEntriesRequest,
  PluginModule,
  RegistryExtractionSummary,
  RegistryStructuredSummary,
  V2GovernanceSnapshot,
  V3GovernanceSnapshot,
  WeChatKeyRecoveryResult,
} from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getSystemInfo(dataSourceId: string): Promise<AnalysisSystemInfo> {
  return apiClient.request(COMMANDS.analysis.GET_SYSTEM_INFO, { request: { dataSourceId } });
}

export async function getFileClassificationBoard(
  dataSourceId: string,
  magicLimit = 300,
): Promise<FileClassificationBoard> {
  return apiClient.request(COMMANDS.analysis.GET_FILE_CLASSIFICATION_BOARD, {
    request: { dataSourceId, sampleSize: magicLimit },
  });
}

export async function getEvidenceClassificationSummary(dataSourceId: string): Promise<EvidenceClassificationSummary> {
  return apiClient.request(COMMANDS.analysis.GET_EVIDENCE_CLASSIFICATION_SUMMARY, { request: { dataSourceId } });
}

export async function runEvidenceClassification(
  dataSourceId: string,
  categories: string[] = [],
): Promise<EvidenceClassificationSummary> {
  return apiClient.request(COMMANDS.analysis.RUN_EVIDENCE_CLASSIFICATION, {
    request: { dataSourceId, categories },
  });
}

export async function runAnalysisExtraction(
  request: AnalysisExtractionRequest,
): Promise<AnalysisExtractionRun> {
  return apiClient.request(COMMANDS.analysis.RUN_ANALYSIS_EXTRACTION, { request });
}

export async function getRegistryExtractionSummary(
  request: AnalysisExtractionPageRequest,
): Promise<RegistryExtractionSummary> {
  return apiClient.request(COMMANDS.analysis.GET_REGISTRY_EXTRACTION_SUMMARY, { request });
}

export async function getRegistryStructuredSummary(dataSourceId: string): Promise<RegistryStructuredSummary> {
  return apiClient.request(COMMANDS.analysis.GET_REGISTRY_STRUCTURED_SUMMARY, { request: { dataSourceId } });
}

export async function getBrowserHistorySummary(
  request: AnalysisExtractionPageRequest,
): Promise<BrowserHistorySummary> {
  return apiClient.request(COMMANDS.analysis.GET_BROWSER_HISTORY_SUMMARY, { request });
}

export async function getEmailExtractionSummary(
  request: AnalysisExtractionPageRequest,
): Promise<EmailExtractionSummary> {
  return apiClient.request(COMMANDS.analysis.GET_EMAIL_EXTRACTION_SUMMARY, { request });
}

export async function getEvtxEventSummary(
  request: EvtxEventPageRequest,
): Promise<EvtxEventSummary> {
  return apiClient.request(COMMANDS.analysis.GET_EVTX_EVENT_SUMMARY, { request });
}

export async function getLinuxArtifactSummary(
  request: AnalysisExtractionPageRequest,
): Promise<LinuxArtifactSummary> {
  return apiClient.request(COMMANDS.analysis.GET_LINUX_ARTIFACT_SUMMARY, { request });
}

export async function getV2GovernanceSnapshot(): Promise<V2GovernanceSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_V2_GOVERNANCE_SNAPSHOT);
}

export async function getV3GovernanceSnapshot(): Promise<V3GovernanceSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_V3_GOVERNANCE_SNAPSHOT);
}

export async function getCaseOverviewSnapshot(): Promise<CaseOverviewSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_CASE_OVERVIEW_SNAPSHOT);
}

export async function listPluginModules(dataSourceId: string): Promise<PluginModule[]> {
  return apiClient.request(COMMANDS.analysis.LIST_PLUGIN_MODULES, { request: { dataSourceId } });
}

export async function getPluginFamilyEntries(
  request: PluginFamilyEntriesRequest,
): Promise<PluginFamilyEntries> {
  return apiClient.request(COMMANDS.analysis.GET_PLUGIN_FAMILY_ENTRIES, { request });
}

export async function listPluginActions(pluginId: string): Promise<PluginActionDescriptor[]> {
  return apiClient.request(COMMANDS.analysis.LIST_PLUGIN_ACTIONS, { request: { pluginId } });
}

export async function recoverWeChatKeys(
  dataSourceId: string,
  dumpPath: string,
): Promise<WeChatKeyRecoveryResult> {
  return apiClient.request(COMMANDS.analysis.RECOVER_WECHAT_KEYS, {
    request: { dataSourceId, dumpPath },
  });
}

export async function getCorrelationSnapshot(): Promise<CorrelationSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_CORRELATION_SNAPSHOT);
}

export async function generateAnalysisSummary(dataSourceId: string): Promise<string> {
  return apiClient.request(COMMANDS.analysis.GENERATE_ANALYSIS_SUMMARY, { request: { dataSourceId } });
}
