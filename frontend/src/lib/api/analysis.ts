import {
  AnalysisExtractionPageRequest,
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  AnalysisFileClassification,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  EmailExtractionSummary,
  EvidenceClassificationSummary,
  RegistryExtractionSummary,
} from '@/types/models';
import { apiClient } from './client';

export async function getSystemInfo(): Promise<AnalysisSystemInfo> {
  return apiClient.request(
    'get_system_info',
    () => apiClient.getMockProvider().getSystemInfo(),
  );
}

export async function classifyFiles(sampleSize = 1000): Promise<AnalysisFileClassification[]> {
  return apiClient.request(
    'classify_files',
    () => apiClient.getMockProvider().classifyFiles(sampleSize),
    { request: { sampleSize } },
  );
}

export async function getEvidenceClassificationSummary(): Promise<EvidenceClassificationSummary> {
  return apiClient.request(
    'get_evidence_classification_summary',
    () => apiClient.getMockProvider().getEvidenceClassificationSummary(),
  );
}

export async function runEvidenceClassification(categories: string[] = []): Promise<EvidenceClassificationSummary> {
  return apiClient.request(
    'run_evidence_classification',
    () => apiClient.getMockProvider().runEvidenceClassification(categories),
    { request: { categories } },
  );
}

export async function runAnalysisExtraction(
  request: AnalysisExtractionRequest = { categories: [] },
): Promise<AnalysisExtractionRun> {
  return apiClient.request(
    'run_analysis_extraction',
    () => apiClient.getMockProvider().runAnalysisExtraction(request),
    { request },
  );
}

export async function getRegistryExtractionSummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<RegistryExtractionSummary> {
  return apiClient.request(
    'get_registry_extraction_summary',
    () => apiClient.getMockProvider().getRegistryExtractionSummary(request),
    { request },
  );
}

export async function getBrowserHistorySummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<BrowserHistorySummary> {
  return apiClient.request(
    'get_browser_history_summary',
    () => apiClient.getMockProvider().getBrowserHistorySummary(request),
    { request },
  );
}

export async function getEmailExtractionSummary(
  request: AnalysisExtractionPageRequest = {},
): Promise<EmailExtractionSummary> {
  return apiClient.request(
    'get_email_extraction_summary',
    () => apiClient.getMockProvider().getEmailExtractionSummary(request),
    { request },
  );
}

export async function generateAnalysisSummary(): Promise<string> {
  return apiClient.request(
    'generate_analysis_summary',
    () => apiClient.getMockProvider().generateAnalysisSummary(),
  );
}
