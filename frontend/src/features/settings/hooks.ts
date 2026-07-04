import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getAppSettings, saveAppSettings } from '@/lib/api/settings';
import type { AppSettings } from '@/types/models';

export function useAppSettings() {
  return useQuery({
    queryKey: ['settings', 'app'],
    queryFn: getAppSettings,
    retry: false,
  });
}

export function useSaveAppSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: AppSettings) => saveAppSettings(settings),
    onSuccess: (saved) => {
      qc.setQueryData(['settings', 'app'], saved);
    },
  });
}
