import { useCallback, useState } from 'react';
import { toast } from 'sonner';
import i18n from '@/i18n';
import { useCurrentCase } from '@/features/case/hooks';
import { useExportReport, useReportHistory, useSaveReport } from './hooks';
import { DEFAULT_REPORT_SCOPE, REPORT_FORMATS } from './report-config';
import type { ExportReportRequest, ReportFormat, ReportHistoryItem, ReportScope } from '@/types/models';

export function useReportsWorkspaceModel() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id;
  const historyQuery = useReportHistory(caseId);
  const exportMutation = useExportReport();
  const saveMutation = useSaveReport();
  const [selectedFormat, setSelectedFormat] = useState<ReportFormat>('html');
  const [exportScope, setExportScope] = useState<ReportScope>(DEFAULT_REPORT_SCOPE);
  const history = historyQuery.data ?? [];
  const runningCount = history.filter((item) => item.status === 'running').length;
  const completedCount = history.filter((item) => item.status === 'completed').length;
  const downloadReport = useCallback(
    (report: ReportHistoryItem) => {
      if (caseId) saveMutation.mutate({ caseId, report });
    },
    [caseId, saveMutation],
  );
  const generate = useCallback(
    (request: ExportReportRequest) => {
      exportMutation.mutate(request, {
        onSuccess: downloadReport,
        onError: (error: Error) => toast.error(i18n.t('reports.exportFailed'), { description: error.message }),
      });
    },
    [downloadReport, exportMutation],
  );
  const runExport = useCallback(() => {
    if (caseId) generate({ caseId, format: selectedFormat, scope: exportScope });
  }, [caseId, exportScope, generate, selectedFormat]);
  const retryReport = useCallback(
    (report: ReportHistoryItem) => {
      if (caseId) generate({ ...report.request, caseId });
    },
    [caseId, generate],
  );
  const selectFormat = useCallback((value: string) => {
    if (REPORT_FORMATS.includes(value as ReportFormat)) setSelectedFormat(value as ReportFormat);
  }, []);
  const setScopeOption = useCallback((option: keyof ReportScope, checked: boolean) => {
    setExportScope((current) => ({ ...current, [option]: checked }));
  }, []);
  return {
    completedCount,
    runningCount,
    history,
    exportScope,
    selectedFormat,
    selectFormat,
    setScopeOption,
    runExport,
    downloadReport,
    retryReport,
    historyError: historyQuery.error,
    exportPending: exportMutation.isPending || !caseId,
    savePending: saveMutation.isPending,
  };
}

export type ReportsWorkspaceModel = ReturnType<typeof useReportsWorkspaceModel>;
