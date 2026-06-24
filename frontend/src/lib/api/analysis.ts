import {
  AnalysisExtractionPageRequest,
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  AnalysisFileClassification,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  CorrelationSnapshot,
  EmailExtractionSummary,
  EvidenceClassificationSummary,
  RegistryExtractionSummary,
  RegistryStructuredSummary,
  V2GovernanceSnapshot,
  V3GovernanceSnapshot,
} from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getSystemInfo(): Promise<AnalysisSystemInfo> {
  return apiClient.request(COMMANDS.analysis.GET_SYSTEM_INFO);
}

export async function classifyFiles(sampleSize = 1000): Promise<AnalysisFileClassification[]> {
  return apiClient.request(COMMANDS.analysis.CLASSIFY_FILES, { request: { sampleSize } });
}

export async function getEvidenceClassificationSummary(): Promise<EvidenceClassificationSummary> {
  return apiClient.request(COMMANDS.analysis.GET_EVIDENCE_CLASSIFICATION_SUMMARY);
}

export async function runEvidenceClassification(categories: string[] = []): Promise<EvidenceClassificationSummary> {
  return apiClient.request(COMMANDS.analysis.RUN_EVIDENCE_CLASSIFICATION, { request: { categories } });
}

export async function runAnalysisExtraction(
  request: AnalysisExtractionRequest = { categories: [] },
): Promise<AnalysisExtractionRun> {
  return apiClient.request(COMMANDS.analysis.RUN_ANALYSIS_EXTRACTION, { request });
}

export async function getRegistryExtractionSummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<RegistryExtractionSummary> {
  return apiClient.request(COMMANDS.analysis.GET_REGISTRY_EXTRACTION_SUMMARY, { request });
}

export async function getRegistryStructuredSummary(): Promise<RegistryStructuredSummary> {
  return apiClient.request(COMMANDS.analysis.GET_REGISTRY_STRUCTURED_SUMMARY);
}

export async function getBrowserHistorySummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<BrowserHistorySummary> {
  return apiClient.request(COMMANDS.analysis.GET_BROWSER_HISTORY_SUMMARY, { request });
}

export async function getEmailExtractionSummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<EmailExtractionSummary> {
  return apiClient.request(COMMANDS.analysis.GET_EMAIL_EXTRACTION_SUMMARY, { request });
}

export async function getV2GovernanceSnapshot(): Promise<V2GovernanceSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_V2_GOVERNANCE_SNAPSHOT);
}

export async function getV3GovernanceSnapshot(): Promise<V3GovernanceSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_V3_GOVERNANCE_SNAPSHOT);
}

export async function getCorrelationSnapshot(): Promise<CorrelationSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_CORRELATION_SNAPSHOT);
}

export async function generateAnalysisSummary(): Promise<string> {
  return apiClient.request(COMMANDS.analysis.GENERATE_ANALYSIS_SUMMARY);
}
