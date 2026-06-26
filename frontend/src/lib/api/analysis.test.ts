import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  classifyFiles,
  generateAnalysisSummary,
  getBrowserCookiesSummary,
  getBrowserHistorySummary,
  getBrowserPasswordsSummary,
  getBrowserSessionsSummary,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEventLogSummary,
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

  it('getSystemInfo calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getSystemInfo();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_SYSTEM_INFO);
  });

  it('classifyFiles sends sampleSize in request payload', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await classifyFiles(500);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.CLASSIFY_FILES, {
      request: { sampleSize: 500 },
    });
  });

  it('classifyFiles defaults sampleSize to 1000', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await classifyFiles();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.CLASSIFY_FILES, {
      request: { sampleSize: 1000 },
    });
  });

  it('getEvidenceClassificationSummary calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getEvidenceClassificationSummary();
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_EVIDENCE_CLASSIFICATION_SUMMARY,
    );
  });

  it('runEvidenceClassification sends categories in request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await runEvidenceClassification(['browser', 'registry']);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.RUN_EVIDENCE_CLASSIFICATION, {
      request: { categories: ['browser', 'registry'] },
    });
  });

  it('runEvidenceClassification defaults categories to empty array', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await runEvidenceClassification();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.RUN_EVIDENCE_CLASSIFICATION, {
      request: { categories: [] },
    });
  });

  it('runAnalysisExtraction sends the request payload', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    const req = { categories: ['registry'] };
    await runAnalysisExtraction(req);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.RUN_ANALYSIS_EXTRACTION, {
      request: req,
    });
  });

  it('getRegistryExtractionSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getRegistryExtractionSummary({ offset: 10, limit: 20 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_REGISTRY_EXTRACTION_SUMMARY,
      { request: { offset: 10, limit: 20 } },
    );
  });

  it('getRegistryStructuredSummary calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getRegistryStructuredSummary();
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_REGISTRY_STRUCTURED_SUMMARY,
    );
  });

  it('getBrowserHistorySummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getBrowserHistorySummary({ offset: 5 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_BROWSER_HISTORY_SUMMARY,
      { request: { offset: 5 } },
    );
  });

  it('getBrowserCookiesSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getBrowserCookiesSummary({});
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_BROWSER_COOKIES_SUMMARY,
      { request: {} },
    );
  });

  it('getBrowserSessionsSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getBrowserSessionsSummary({ limit: 50 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_BROWSER_SESSIONS_SUMMARY,
      { request: { limit: 50 } },
    );
  });

  it('getBrowserPasswordsSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getBrowserPasswordsSummary({});
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_BROWSER_PASSWORDS_SUMMARY,
      { request: {} },
    );
  });

  it('getEmailExtractionSummary sends page request', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getEmailExtractionSummary({ offset: 3 });
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.analysis.GET_EMAIL_EXTRACTION_SUMMARY,
      { request: { offset: 3 } },
    );
  });

  it('getEventLogSummary calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getEventLogSummary();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_EVENT_LOG_SUMMARY);
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
    const result = await generateAnalysisSummary();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GENERATE_ANALYSIS_SUMMARY);
    expect(result).toBe('summary text');
  });
});
