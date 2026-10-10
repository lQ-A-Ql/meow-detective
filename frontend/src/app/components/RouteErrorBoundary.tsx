import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { RefreshCw } from 'lucide-react';
import { isRouteErrorResponse, useRouteError } from 'react-router';
import { Button } from '@/app/components/ui/button';

/**
 * Router-level counterpart to the React ErrorBoundary: React Router renders this for a loader
 * or render failure inside the matched route, so one broken page shows a contained message
 * instead of replacing the whole application with the development overlay.
 */
export function RouteErrorBoundary() {
  const { t } = useTranslation();
  const error = useRouteError();

  const message = isRouteErrorResponse(error)
    ? `${error.status} ${error.statusText}`
    : error instanceof Error
      ? error.message
      : t('errorBoundary.unknownError');

  return (
    <div className="flex min-h-[50vh] items-center justify-center bg-forensics-surface p-6">
      <div className="w-full max-w-lg border border-forensics-border bg-forensics-panel p-8 text-center">
        <div className="mb-3 font-serif text-2xl text-forensics-text">{t('errorBoundary.title')}</div>
        <div className="mb-4 font-mono text-[13px] leading-6 break-all text-forensics-muted">{message}</div>
        <Tooltip content={t('errorBoundary.reload')}><Button aria-label={t('errorBoundary.reload')} type="button" variant="forensicsSurface" size="sm" onClick={() => window.location.reload()}><ActionIcon name="refresh" /></Button></Tooltip>
      </div>
    </div>
  );
}
