import {
  AnalysisFileClassification,
  AnalysisSystemInfo,
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

export async function generateAnalysisSummary(): Promise<string> {
  return apiClient.request(
    'generate_analysis_summary',
    () => apiClient.getMockProvider().generateAnalysisSummary(),
  );
}
