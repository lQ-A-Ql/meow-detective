import {
  AnalysisExtractionPageRequest,
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  AnalysisFileClassification,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  CorrelationSnapshot,
  EmailExtractionSummary,
  EvtxEventSummary,
  EvidenceClassificationSummary,
  LinuxArtifactSummary,
  RegistryExtractionSummary,
  RegistryStructuredSummary,
  V2GovernanceSnapshot,
  V3GovernanceSnapshot,
} from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getSystemInfo(dataSourceId: string): Promise<AnalysisSystemInfo> {
  return apiClient.request(COMMANDS.analysis.GET_SYSTEM_INFO, { request: { dataSourceId } });
}

export async function classifyFiles(
  dataSourceId: string,
  sampleSize = 1000,
): Promise<AnalysisFileClassification[]> {
  return apiClient.request(COMMANDS.analysis.CLASSIFY_FILES, { request: { dataSourceId, sampleSize } });
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
  request: AnalysisExtractionPageRequest,
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

export async function getCorrelationSnapshot(): Promise<CorrelationSnapshot> {
  return apiClient.request(COMMANDS.analysis.GET_CORRELATION_SNAPSHOT);
}

export async function generateAnalysisSummary(dataSourceId: string): Promise<string> {
  return apiClient.request(COMMANDS.analysis.GENERATE_ANALYSIS_SUMMARY, { request: { dataSourceId } });
}
