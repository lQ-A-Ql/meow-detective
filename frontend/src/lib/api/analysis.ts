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
import { apiClient } from './client';

export async function getSystemInfo(): Promise<AnalysisSystemInfo> {
  return apiClient.request('get_system_info');
}

export async function classifyFiles(sampleSize = 1000): Promise<AnalysisFileClassification[]> {
  return apiClient.request('classify_files', { request: { sampleSize } });
}

export async function getEvidenceClassificationSummary(): Promise<EvidenceClassificationSummary> {
  return apiClient.request('get_evidence_classification_summary');
}

export async function runEvidenceClassification(categories: string[] = []): Promise<EvidenceClassificationSummary> {
  return apiClient.request('run_evidence_classification', { request: { categories } });
}

export async function runAnalysisExtraction(
  request: AnalysisExtractionRequest = { categories: [] },
): Promise<AnalysisExtractionRun> {
  return apiClient.request('run_analysis_extraction', { request });
}

export async function getRegistryExtractionSummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<RegistryExtractionSummary> {
  return apiClient.request('get_registry_extraction_summary', { request });
}

export async function getRegistryStructuredSummary(): Promise<RegistryStructuredSummary> {
  return apiClient.request('get_registry_structured_summary');
}

export async function getBrowserHistorySummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<BrowserHistorySummary> {
  return apiClient.request('get_browser_history_summary', { request });
}

export async function getEmailExtractionSummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<EmailExtractionSummary> {
  return apiClient.request('get_email_extraction_summary', { request });
}

export async function getV2GovernanceSnapshot(): Promise<V2GovernanceSnapshot> {
  return apiClient.request('get_v2_governance_snapshot');
}

export async function getV3GovernanceSnapshot(): Promise<V3GovernanceSnapshot> {
  return apiClient.request('get_v3_governance_snapshot');
}

export async function getCorrelationSnapshot(): Promise<CorrelationSnapshot> {
  return apiClient.request('get_correlation_snapshot');
}

export async function generateAnalysisSummary(): Promise<string> {
  return apiClient.request('generate_analysis_summary');
}
