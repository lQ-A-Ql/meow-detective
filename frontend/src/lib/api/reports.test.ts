import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  exportCsvCorrelationReport,
  exportCsvReport,
  exportHtmlReport,
  exportJsonReport,
  getReportHistory,
  getReportTemplates,
} from './reports';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('reports API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getReportTemplates calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getReportTemplates();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.GET_REPORT_TEMPLATES);
  });

  it('getReportHistory calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getReportHistory();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.GET_REPORT_HISTORY);
  });

  it('exportHtmlReport sends scope with overwrite defaulting to false', async () => {
    requestMock.mockResolvedValueOnce('/reports/out.html' as never);
    const result = await exportHtmlReport({ fileSystemMetadata: true, registry: true, fullTimeline: true, rawFileExtraction: false });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.EXPORT_HTML_REPORT, {
      scope: { fileSystemMetadata: true, registry: true, fullTimeline: true, rawFileExtraction: false, overwrite: false },
    });
    expect(result).toBe('/reports/out.html');
  });

  it('exportHtmlReport sends undefined scope when omitted', async () => {
    requestMock.mockResolvedValueOnce('/reports/out.html' as never);
    await exportHtmlReport();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.EXPORT_HTML_REPORT, {
      scope: undefined,
    });
  });

  it('exportHtmlReport honors overwrite option', async () => {
    requestMock.mockResolvedValueOnce('/reports/out.html' as never);
    await exportHtmlReport({ fileSystemMetadata: true, registry: true, fullTimeline: true, rawFileExtraction: false }, { overwrite: true });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.EXPORT_HTML_REPORT, {
      scope: { fileSystemMetadata: true, registry: true, fullTimeline: true, rawFileExtraction: false, overwrite: true },
    });
  });

  it('exportCsvReport sends scope with overwrite', async () => {
    requestMock.mockResolvedValueOnce('/reports/out.csv' as never);
    await exportCsvReport({ fileSystemMetadata: true, registry: false, fullTimeline: true, rawFileExtraction: false });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.EXPORT_CSV_REPORT, {
      scope: { fileSystemMetadata: true, registry: false, fullTimeline: true, rawFileExtraction: false, overwrite: false },
    });
  });

  it('exportJsonReport sends scope with overwrite', async () => {
    requestMock.mockResolvedValueOnce('/reports/out.json' as never);
    await exportJsonReport({ fileSystemMetadata: false, registry: true, fullTimeline: false, rawFileExtraction: true });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.EXPORT_JSON_REPORT, {
      scope: { fileSystemMetadata: false, registry: true, fullTimeline: false, rawFileExtraction: true, overwrite: false },
    });
  });

  it('exportCsvCorrelationReport sends scope with overwrite', async () => {
    requestMock.mockResolvedValueOnce('/reports/corr.csv' as never);
    await exportCsvCorrelationReport(
      { fileSystemMetadata: false, registry: true, fullTimeline: true, rawFileExtraction: false },
      { overwrite: true },
    );
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.reports.EXPORT_CSV_CORRELATION_REPORT, {
      scope: { fileSystemMetadata: false, registry: true, fullTimeline: true, rawFileExtraction: false, overwrite: true },
    });
  });
});
