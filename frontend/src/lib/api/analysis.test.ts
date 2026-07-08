import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  classifyFiles,
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEvidenceClassificationSummary,
  getRegistryExtractionSummary,
  getRegistryStructuredSummary,
  getSystemInfo,
  getV2GovernanceSnapshot,
  getV3GovernanceSnapshot,
  runAnalysisExtraction,
  runEvidenceClassification,
} from './analysis';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('analysis API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getSystemInfo sends dataSourceId', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getSystemInfo('ds-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_SYSTEM_INFO, {
      request: { dataSourceId: 'ds-1' },
    });
  });

  it('classifyFiles sends sampleSize in request payload', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await classifyFiles('ds-1', 500);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.CLASSIFY_FILES, {
      request: { dataSourceId: 'ds-1', sampleSize: 500 },
    });
  });

  it('classifyFiles defaults sampleSize to 1000', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await classifyFiles('ds-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.CLASSIFY_FILES, {
      request: { dataSourceId: 'ds-1', sampleSize: 1000 },
    });
  });

  it('getEvidenceClassificationSummary calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getEvidenceClassificationSummary('ds-1');
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_EVIDENCE_CLASSIFICATION_SUMMARY,
      { request: { dataSourceId: 'ds-1' } },
    );
  });

  it('runEvidenceClassification sends categories in request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await runEvidenceClassification('ds-1', ['browser', 'registry']);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.RUN_EVIDENCE_CLASSIFICATION, {
      request: { dataSourceId: 'ds-1', categories: ['browser', 'registry'] },
    });
  });

  it('runEvidenceClassification defaults categories to empty array', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await runEvidenceClassification('ds-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.RUN_EVIDENCE_CLASSIFICATION, {
      request: { dataSourceId: 'ds-1', categories: [] },
    });
  });

  it('runAnalysisExtraction sends the request payload', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    const req = { dataSourceId: 'ds-1', categories: ['registry'] };
    await runAnalysisExtraction(req);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.RUN_ANALYSIS_EXTRACTION, {
      request: req,
    });
  });

  it('getRegistryExtractionSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getRegistryExtractionSummary({ dataSourceId: 'ds-1', offset: 10, limit: 20 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_REGISTRY_EXTRACTION_SUMMARY,
      { request: { dataSourceId: 'ds-1', offset: 10, limit: 20 } },
    );
  });

  it('getRegistryStructuredSummary calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getRegistryStructuredSummary('ds-1');
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_REGISTRY_STRUCTURED_SUMMARY,
      { request: { dataSourceId: 'ds-1' } },
    );
  });

  it('getBrowserHistorySummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getBrowserHistorySummary({ dataSourceId: 'ds-1', offset: 5 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_BROWSER_HISTORY_SUMMARY,
      { request: { dataSourceId: 'ds-1', offset: 5 } },
    );
  });

  it('getEmailExtractionSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getEmailExtractionSummary({ dataSourceId: 'ds-1', offset: 3 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_EMAIL_EXTRACTION_SUMMARY,
      { request: { dataSourceId: 'ds-1', offset: 3 } },
    );
  });

  it('getV2GovernanceSnapshot calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getV2GovernanceSnapshot();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_V2_GOVERNANCE_SNAPSHOT);
  });

  it('getV3GovernanceSnapshot calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getV3GovernanceSnapshot();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_V3_GOVERNANCE_SNAPSHOT);
  });

  it('getCorrelationSnapshot calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getCorrelationSnapshot();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_CORRELATION_SNAPSHOT);
  });

  it('generateAnalysisSummary calls the correct command', async () => {
    requestMock.mockResolvedValueOnce('summary text' as never);
    const result = await generateAnalysisSummary('ds-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GENERATE_ANALYSIS_SUMMARY, {
      request: { dataSourceId: 'ds-1' },
    });
    expect(result).toBe('summary text');
  });
});
