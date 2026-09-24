import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  getFileClassificationBoard,
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getCaseOverviewSnapshot,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEvidenceClassificationSummary,
  getLinuxEvidenceSetSummary,
  getLinuxEvidenceEvents,
  listLinuxEvidenceSets,
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

  it('getFileClassificationBoard sends magic limit in request payload', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getFileClassificationBoard('ds-1', 500);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_FILE_CLASSIFICATION_BOARD, {
      request: { dataSourceId: 'ds-1', sampleSize: 500 },
    });
  });

  it('getFileClassificationBoard defaults magic limit to 300', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getFileClassificationBoard('ds-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_FILE_CLASSIFICATION_BOARD, {
      request: { dataSourceId: 'ds-1', sampleSize: 300 },
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

  it('getCaseOverviewSnapshot calls the dedicated overview command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getCaseOverviewSnapshot();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.GET_CASE_OVERVIEW_SNAPSHOT);
  });

  it('lists Linux evidence sets for the active case', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await listLinuxEvidenceSets();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.analysis.LIST_LINUX_EVIDENCE_SETS, { request: {} });
  });

  it('loads Linux evidence set summary and scoped events', async () => {
    requestMock.mockResolvedValueOnce({} as never).mockResolvedValueOnce([] as never);
    await getLinuxEvidenceSetSummary('set-1');
    await getLinuxEvidenceEvents('set-1', 10, 25);
    expect(requestMock).toHaveBeenNthCalledWith(1, COMMANDS.analysis.GET_LINUX_EVIDENCE_SET_SUMMARY, { request: { importSetId: 'set-1' } });
    expect(requestMock).toHaveBeenNthCalledWith(2, COMMANDS.analysis.GET_LINUX_EVIDENCE_EVENTS, { request: { importSetId: 'set-1', offset: 10, limit: 25 } });
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
