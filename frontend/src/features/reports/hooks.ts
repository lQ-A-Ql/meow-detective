import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import i18n from '@/i18n';
import { exportReport, getReportHistory, saveReport } from '@/lib/api/reports';
import { saveDialog } from '@/lib/platform/dialog';
import type { ExportReportRequest, ReportHistoryItem } from '@/types/models';

export function useReportHistory(caseId?: string) {
  const exporting = useIsMutating({ mutationKey: ['reports', 'export'] });
  return useQuery({
    queryKey: ['reports', 'history', caseId ?? null],
    queryFn: () => getReportHistory(caseId!),
    enabled: Boolean(caseId),
    refetchInterval: (query) =>
      exporting > 0 || query.state.data?.some((item) => item.status === 'running') ? 1000 : false,
  });
}

export function useExportReport() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationKey: ['reports', 'export'],
    mutationFn: (request: ExportReportRequest) => exportReport(request),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['reports'] }),
  });
}

export function useSaveReport() {
  return useMutation({
    mutationFn: async ({ caseId, report }: { caseId: string; report: ReportHistoryItem }) => {
      const destination = await saveDialog({ defaultPath: report.fileName });
      if (!destination) return false;
      await saveReport({ caseId, reportId: report.id, destination, overwrite: true });
      return true;
    },
    onSuccess: (saved) => {
      if (saved) toast.success(i18n.t('reports.saveSuccess'));
    },
    onError: (error: Error) => toast.error(i18n.t('reports.saveFailed'), { description: error.message }),
  });
}
