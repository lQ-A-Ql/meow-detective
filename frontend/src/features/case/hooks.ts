import { useQuery } from '@tanstack/react-query';
import { getCaseMetrics, getCurrentCase, getRecentObjects } from '@/lib/api/case';

export function useCurrentCase() {
  return useQuery({ queryKey: ['case', 'current'], queryFn: getCurrentCase });
}

export function useCaseMetrics() {
  return useQuery({ queryKey: ['case', 'metrics'], queryFn: getCaseMetrics });
}

export function useRecentObjects() {
  return useQuery({ queryKey: ['case', 'recent-objects'], queryFn: getRecentObjects });
}
