import { useCallback, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { useDataSources } from '@/features/case/hooks';
import {
  deriveEvidenceHashStatus,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  useImportEventState,
} from '@/features/jobs/import-event-state';
import {
  useExportReport,
  useReportHistory,
  useReportTemplates,
  type ReportExportFormat,
} from '@/features/reports/hooks';
import type { ExportScope } from '@/types/models';

const REPORT_EXPORT_FORMATS: readonly ReportExportFormat[] = ['html', 'csv', 'json'];

function isReportExportFormat(value: string): value is ReportExportFormat {
  return REPORT_EXPORT_FORMATS.includes(value as ReportExportFormat);
}

/** Owns report queries, export scope state, and export feedback. */
export function useReportsWorkspaceModel() {
  const templatesQuery = useReportTemplates();
  const historyQuery = useReportHistory();
  const dataSourcesQuery = useDataSources();
  const importSignals = useImportEventState();
  const exportMutation = useExportReport();
  const [selectedFormat, setSelectedFormat] = useState<ReportExportFormat>('html');
  const [exportScope, setExportScope] = useState<ExportScope>({
    fileSystemMetadata: true,
    registry: true,
    fullTimeline: true,
    rawFileExtraction: false,
  });
  const history = historyQuery.data;
  const runningCount = useMemo(
    () => history?.filter((entry) => entry.status === 'running').length ?? 0,
    [history],
  );
  const completedCount = useMemo(
    () => history?.filter((entry) => entry.status === 'completed').length ?? 0,
    [history],
  );
  const evidenceHashStatus = useMemo(
    () => deriveEvidenceHashStatus(importSignals.partialResults, dataSourcesQuery.data ?? []),
    [dataSourcesQuery.data, importSignals.partialResults],
  );
  const selectFormat = useCallback((value: string) => {
    if (isReportExportFormat(value)) {
      setSelectedFormat(value);
    }
  }, []);
  const setScopeOption = useCallback((option: keyof ExportScope, checked: boolean) => {
    setExportScope((current) => ({ ...current, [option]: checked }));
  }, []);
  const runExport = useCallback(() => {
    exportMutation.mutate(
      { format: selectedFormat, scope: exportScope },
      {
        onSuccess: (outputPath) => {
          toast.success('报告生成成功', { description: outputPath });
        },
        onError: (error: Error) => {
          toast.error('报告生成失败', { description: error.message });
        },
      },
    );
  }, [exportMutation, exportScope, selectedFormat]);

  return {
    completedCount,
    evidenceHashCaveat: evidenceHashStatus ? getEvidenceHashCaveatText(evidenceHashStatus) : undefined,
    evidenceHashLabel: evidenceHashStatus ? getEvidenceHashStatusLabel(evidenceHashStatus) : undefined,
    exportPending: exportMutation.isPending,
    exportScope,
    history,
    reportTemplates: templatesQuery.data,
    runningCount,
    runExport,
    selectFormat,
    selectedFormat,
    setScopeOption,
  };
}

export type ReportsWorkspaceModel = ReturnType<typeof useReportsWorkspaceModel>;
