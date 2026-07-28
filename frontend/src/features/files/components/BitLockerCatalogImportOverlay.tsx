import { LoaderCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Card, CardContent, CardHeader, CardTitle } from '@/app/components/ui/card';
import { Progress } from '@/app/components/ui/progress';
import type { BitLockerCatalogImportLifecycle } from '@/features/files/hooks/use-bitlocker-volume';

interface BitLockerCatalogImportOverlayProps {
  lifecycle: BitLockerCatalogImportLifecycle;
}

function useElapsedSeconds(startedAt: number) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [startedAt]);

  return Math.max(0, Math.floor((now - startedAt) / 1_000));
}

export function BitLockerCatalogImportOverlay({ lifecycle }: BitLockerCatalogImportOverlayProps) {
  const { t } = useTranslation();
  const elapsedSeconds = useElapsedSeconds(lifecycle.startedAt);
  const isRefreshing = lifecycle.phase === 'refreshing';
  const phaseLabel = isRefreshing
    ? t('fileBrowser.bitlockerImport.refreshing')
    : t('fileBrowser.bitlockerImport.cataloging');

  return (
    <div
      className="absolute inset-0 z-40 grid place-items-center bg-forensics-surface/90 p-6 backdrop-blur-[1px]"
      role="status"
      aria-live="assertive"
      aria-label={t('fileBrowser.bitlockerImport.ariaLabel')}
      data-testid="bitlocker-catalog-import-overlay"
    >
      <Card className="w-full max-w-md border-forensics-border-strong bg-forensics-panel">
        <CardHeader className="gap-2 px-5 pt-5">
          <CardTitle className="flex items-center gap-2 text-sm font-medium text-forensics-text">
            <LoaderCircle size={16} className="animate-spin text-forensics-primary-blue" aria-hidden="true" />
            {t('fileBrowser.bitlockerImport.title')}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 px-5 pb-5">
          <div className="text-[12px] text-forensics-text-secondary">{phaseLabel}</div>
          <Progress
            indeterminate
            aria-label={phaseLabel}
            className="h-1.5"
          />
          <div className="font-mono text-[10px] text-forensics-muted-light">
            {t('fileBrowser.bitlockerImport.elapsed', { seconds: elapsedSeconds })}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
