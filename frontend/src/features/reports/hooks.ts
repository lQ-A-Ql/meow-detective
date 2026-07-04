import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  exportCsvReport,
  exportHtmlReport,
  exportJsonReport,
  getReportHistory,
  getReportTemplates,
} from '@/lib/api/reports';
import type { ExportScope } from '@/types/models';

export function useReportTemplates() {
  return useQuery({ queryKey: ['reports', 'templates'], queryFn: getReportTemplates });
}

export function useReportHistory() {
  return useQuery({ queryKey: ['reports', 'history'], queryFn: getReportHistory });
}

export type ReportExportFormat = 'html' | 'csv' | 'json';

export function useExportReport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ format, scope }: { format: ReportExportFormat; scope: ExportScope }) => {
      if (format === 'csv') {
        return exportCsvReport(scope);
      }
      if (format === 'json') {
        return exportJsonReport(scope);
      }
      return exportHtmlReport(scope);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['reports'] });
    },
  });
}
