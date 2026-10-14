import { useCallback } from 'react';
import { toast } from 'sonner';
import i18n from '@/i18n';

/** Clipboard effects stay in the logical layer and report denied/unavailable access. */
export function useCopyText(value: string) {
  return useCallback(() => {
    if (!navigator.clipboard) {
      toast.error(i18n.t('common.clipboardUnavailable'));
      return;
    }
    void navigator.clipboard.writeText(value).catch(() => toast.error(i18n.t('common.clipboardUnavailable')));
  }, [value]);
}
