import { useQuery } from '@tanstack/react-query';
import { getJobsSnapshot, getTraceItems, getWarnings } from '@/lib/api/jobs';

export function useJobsSnapshot() {
  return useQuery({ queryKey: ['jobs', 'snapshot'], queryFn: getJobsSnapshot });
}

export function useWarnings() {
  return useQuery({ queryKey: ['jobs', 'warnings'], queryFn: getWarnings });
}

export function useTraceItems() {
  return useQuery({ queryKey: ['jobs', 'trace'], queryFn: getTraceItems });
}
