import { useCallback } from 'react';
import { toast } from 'sonner';
import i18n from '@/i18n';
import { writeClipboardText } from '@/lib/platform/clipboard';

interface CopyTextOptions {
  onSuccess?: () => void;
  onError?: (error: unknown) => void;
}

/** Clipboard effects stay in the logical layer and report denied/unavailable access. */
export function useCopyText(value: string, options: CopyTextOptions = {}) {
  const { onSuccess, onError } = options;
  return useCallback(() => {
    void writeClipboardText(value).then(
      onSuccess,
      onError ?? (() => toast.error(i18n.t('common.clipboardUnavailable'))),
    );
  }, [value, onSuccess, onError]);
}
