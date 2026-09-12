import { useTranslation } from 'react-i18next';
import { AppWindow, Smartphone } from 'lucide-react';
import { Avatar, AvatarFallback, AvatarImage } from '@/app/components/ui/avatar';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/app/components/ui/card';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { EmptyState, KeyValueField, PanelFrame, SectionHeader } from '@/components/data-display';
import {
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
} from '@/features/analysis/components/AnalysisPanels';
import type {
  AndroidAnalysisRun,
  AndroidDeviceInfo,
  AndroidPackageSummary,
} from '@/types/models';
import type { AndroidAnalysisTabKey } from '@/features/analysis/types';

interface AndroidAnalysisViewProps {
  deviceInfo?: AndroidDeviceInfo;
  packageSummary?: AndroidPackageSummary;
  activePanel: AndroidAnalysisTabKey;
  loading: boolean;
  running: boolean;
  lastRun?: AndroidAnalysisRun;
  error?: string;
  hasMore: boolean;
  loadingMore: boolean;
  onRetry: () => void;
  onLoadMore: () => void;
}

export function AndroidAnalysisView({
  deviceInfo,
  packageSummary,
  activePanel,
  loading,
  running,
  lastRun,
  error,
  hasMore,
  loadingMore,
  onRetry,
  onLoadMore,
}: AndroidAnalysisViewProps) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full min-h-0 flex-1 flex-col">
      <ScrollArea className="min-h-0 flex-1" viewportClassName="p-6">
        {error ? <AnalysisErrorBanner message={error} onRetry={onRetry} /> : null}
        {loading || running ? (
          <AnalysisLoadingPanel text={t(running ? 'analysis.android.running' : 'analysis.android.loading')} />
        ) : (
          <>
            {lastRun ? (
              <div className="mb-4">
                <Badge variant="outline">
                  {t('analysis.android.lastRun', {
                    scanned: lastRun.scannedFileCount,
                    facts: lastRun.deviceFactCount,
                    packages: lastRun.packageCount,
                  })}
                </Badge>
              </div>
            ) : null}
            {activePanel === 'device' ? (
              <AndroidDevicePanel deviceInfo={deviceInfo} />
            ) : (
              <AndroidPackagesPanel
                summary={packageSummary}
                hasMore={hasMore}
                loadingMore={loadingMore}
                onLoadMore={onLoadMore}
              />
            )}
          </>
        )}
      </ScrollArea>
    </div>
  );
}

function AndroidDevicePanel({ deviceInfo }: { deviceInfo?: AndroidDeviceInfo }) {
  const { t } = useTranslation();
  const facts = new Map((deviceInfo?.facts ?? []).map((fact) => [fact.field, fact]));
  const fields = [
    'model',
    'manufacturer',
    'brand',
    'device',
    'androidVersion',
    'sdkInt',
    'buildId',
    'buildDisplayId',
    'buildFingerprint',
    'serialNumber',
    'deviceName',
    'androidId',
    'imei',
  ];
  return (
    <div className="space-y-5">
      <SectionHeader
        icon={Smartphone}
        title={t('analysis.android.deviceTitle')}
        subtitle={t('analysis.android.deviceDescription')}
      />
      <PanelFrame>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-3">
          {fields.map((field) => {
            const fact = facts.get(field);
            return (
              <KeyValueField
                key={field}
                label={t('analysis.android.fields.' + field)}
                value={fact?.value}
                fallback=""
                mono={field === 'buildFingerprint' || field === 'androidId' || field === 'imei'}
                valueClassName={field === 'buildFingerprint' ? 'break-all' : undefined}
              />
            );
          })}
        </div>
      </PanelFrame>
    </div>
  );
}

function AndroidPackagesPanel({
  summary,
  hasMore,
  loadingMore,
  onLoadMore,
}: {
  summary?: AndroidPackageSummary;
  hasMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
}) {
  const { t } = useTranslation();
  const packages = summary?.packages ?? [];

  return (
    <div className="space-y-5">
      <SectionHeader
        icon={AppWindow}
        title={t('analysis.android.packagesTitle')}
        subtitle={t('analysis.android.packagesDescription')}
      />
      {packages.length === 0 ? (
        <EmptyState>{t('analysis.android.noPackages')}</EmptyState>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 2xl:grid-cols-3">
          {packages.map((item) => (
            <AndroidPackageCard key={item.packageName} item={item} />
          ))}
        </div>
      )}
      {hasMore ? (
        <div className="flex justify-center">
          <Button type="button" variant="outline" onClick={onLoadMore} disabled={loadingMore}>
            {t('analysis.android.loadMore')}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function AndroidPackageCard({ item }: { item: AndroidPackageSummary['packages'][number] }) {
  const { t } = useTranslation();
  return (
    <Card className="min-w-0 gap-3 rounded-md">
      <CardHeader className="grid grid-cols-[auto_minmax(0,1fr)] gap-3 px-4 pt-4">
        <Avatar className="size-10">
          {item.iconDataUrl ? <AvatarImage src={item.iconDataUrl} alt="" /> : null}
          <AvatarFallback>
            <AppWindow size={16} aria-hidden="true" />
          </AvatarFallback>
        </Avatar>
        <div className="min-w-0">
          <CardTitle className="truncate font-mono text-[12px]" title={item.packageName}>
            {item.packageName}
          </CardTitle>
          <div className="mt-1 truncate text-[11px] text-forensics-muted" title={item.appName}>
            {item.appName ?? ''}
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-2 px-4 pb-4 text-[11px]">
        <div className="grid grid-cols-2 gap-2">
          <KeyValueField
            label={t('analysis.android.versionCode')}
            value={item.versionCode}
            fallback=""
            mono
          />
          <KeyValueField
            label={t('analysis.android.uid')}
            value={item.uid?.toString()}
            fallback=""
            mono
          />
        </div>
        <KeyValueField
          label={t('analysis.android.installTime')}
          value={formatAndroidTime(item.installTime)}
          fallback=""
        />
        <KeyValueField
          label={t('analysis.android.updateTime')}
          value={formatAndroidTime(item.updateTime)}
          fallback=""
        />
        <div className="flex flex-wrap gap-1">
          {item.userState ? (
            <Badge variant="outline">
              {t('analysis.android.userStates.' + item.userState)}
            </Badge>
          ) : null}
          {item.installer ? <Badge variant="outline">{item.installer}</Badge> : null}
        </div>
      </CardContent>
    </Card>
  );
}

function formatAndroidTime(value?: string) {
  if (!value) return undefined;
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.getTime()) ? value : timestamp.toLocaleString();
}
