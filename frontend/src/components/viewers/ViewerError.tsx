import { memo, useCallback } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { AlertCircle } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { useTranslation } from 'react-i18next';
import type { ApiErrorDto } from '@/types/models';

interface ViewerErrorProps {
  /** The error object from the Tauri command */
  error: ApiErrorDto;
  /** Callback to retry the failed operation */
  onRetry?: () => void;
}

function copyErrorToClipboard(error: ApiErrorDto) {
  const lines = [
    `Code: ${error.code}`,
    `Message: ${error.message}`,
    `Category: ${error.category ?? 'unknown'}`,
  ];
  if (error.suggestion) lines.push(`Suggestion: ${error.suggestion}`);
  void navigator.clipboard.writeText(lines.join('\n'));
}

export const ViewerError = memo(function ViewerError({ error, onRetry }: ViewerErrorProps) {
  const { t } = useTranslation();
  const handleCopy = useCallback(() => copyErrorToClipboard(error), [error]);

  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-forensics-surface p-8">
      <div className="max-w-lg text-center">
        <AlertCircle size={36} className="mx-auto mb-3 text-forensics-error-text" />

        <h3 className="mb-2 font-mono text-[13px] font-light text-forensics-text">
          [{error.code}] {t('viewer.error.title')}
        </h3>

        <p className="mb-4 text-[13px] leading-relaxed text-forensics-muted">
          {error.message}
        </p>

        {error.suggestion && (
          <div className="mb-4 rounded-none border border-forensics-warning-border bg-forensics-warning-bg px-4 py-3 text-left">
            <p className="text-[12px] leading-relaxed text-forensics-warning-text">
              💡 {error.suggestion}
            </p>
          </div>
        )}

        <div className="flex items-center justify-center gap-2">
          <Tooltip content={t('viewer.error.copyDetails')}><Button
            type="button"
            variant="forensicsOutline"
            size="xs"
            onClick={handleCopy}
            aria-label={t('viewer.error.copyDetails')}
          ><ActionIcon name="copy" /></Button></Tooltip>
          {error.recoverable && onRetry && (
            <Tooltip content={t('viewer.error.retryPreview')}><Button
              type="button"
              variant="forensicsPrimary"
              size="xs"
              onClick={onRetry}
              aria-label={t('viewer.error.retryPreview')}
            ><ActionIcon name="refresh" /></Button></Tooltip>
          )}
        </div>
      </div>
    </div>
  );
});
