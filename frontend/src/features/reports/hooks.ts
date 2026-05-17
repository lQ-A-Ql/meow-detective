import { useQuery } from '@tanstack/react-query';
import { getReportHistory, getReportTemplates } from '@/lib/api/reports';

export function useReportTemplates() {
  return useQuery({ queryKey: ['reports', 'templates'], queryFn: getReportTemplates });
}

export function useReportHistory() {
  return useQuery({ queryKey: ['reports', 'history'], queryFn: getReportHistory });
}
