import { Clock, Monitor, Network, RefreshCw, Shield } from 'lucide-react';
import { ContextBar } from '@/components/layout/ContextBar';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { BrandEmptyState } from '@/components/brand';
import type {
  AnalysisSystemInfo,
} from '@/types/models';
import {
  EmptyLine,
  FieldProvenancePanel,
  formatProvenanceSummary,
  InfoCard,
  ProvenancePanel,
  StatusPill,
  WarningList,
} from './helpers';

export function SystemInfoPanel({ systemInfo }: { systemInfo?: AnalysisSystemInfo }) {
  const { t } = useTranslation();
  const info = systemInfo ?? {
    networkAdapters: [],
    bootHistory: [],
    status: 'unavailable' as const,
    warnings: [t('analysisPanels.systemInfo.unavailable')],
    provenance: [],
    fieldProvenance: [],
  };
  const parserFailures = info.provenance.filter(
    (item) => item.status !== 'parsed' && item.warnings.length > 0,
  );

  return (
    <div className="space-y-6">
      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Monitor size={16} />
          {t('analysis.tabs.system')}
        </h3>
        <div className="mb-3 flex items-center gap-2 text-[12px] text-forensics-muted">
          <StatusPill status={info.status} />
          {info.warnings[0] ? <span>{info.warnings[0]}</span> : null}
        </div>
        {parserFailures.length > 0 ? (
          <WarningList warnings={[t('analysisPanels.systemInfo.parserFailures')]} />
        ) : null}
        {info.warnings.length > 1 ? <WarningList warnings={info.warnings.slice(0, 3)} /> : null}
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <InfoCard label={t('analysisPanels.systemInfo.computerName')} value={info.computerName} />
          <InfoCard label={t('linuxArtifacts.overview.os')} value={info.osVersion} />
          <InfoCard label={t('analysisPanels.systemInfo.buildNumber')} value={info.buildNumber} />
          <InfoCard label={t('analysisPanels.systemInfo.registeredOwner')} value={info.registeredOwner} />
          <InfoCard label={t('analysisPanels.systemInfo.timezone')} value={info.timezone} />
          <InfoCard label={t('analysis.registry.columns.installDate')} value={info.installDate} />
          <InfoCard label={t('analysisPanels.systemInfo.lastShutdownTime')} value={info.shutdownTime} />
        </div>
      </section>

      <ProvenancePanel
        title={t('analysisPanels.systemInfo.provenanceTitle')}
        provenance={info.provenance}
        fallback={t('analysisPanels.systemInfo.provenanceFallback')}
      />

      <FieldProvenancePanel fieldProvenance={info.fieldProvenance} />

      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Network size={16} />
          {t('analysisPanels.systemInfo.networkAdapters')}
        </h3>
        {info.networkAdapters.length > 0 ? (
          <div className="space-y-2">
            {info.networkAdapters.map((adapter) => (
              <div key={adapter.name} className="rounded-none border border-forensics-border bg-forensics-panel p-3">
                <div className="text-[12px] font-light">{adapter.name}</div>
                <div className="mt-1 font-mono text-[11px] text-forensics-muted">
                  MAC: {adapter.macAddress ?? '-'}
                </div>
                <div className="font-mono text-[11px] text-forensics-muted">
                  IP: {adapter.ipAddresses.join(', ') || '-'}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine text={t('analysisPanels.systemInfo.networkAdaptersEmpty')} />
        )}
      </section>

      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Clock size={16} />
          {t('analysisPanels.systemInfo.bootHistory')}
        </h3>
        {info.bootHistory.length > 0 ? (
          <div className="space-y-1">
            {info.bootHistory.map((boot) => (
              <div key={`${boot.timestamp}-${boot.source}`} className="rounded-none border border-forensics-border bg-forensics-panel p-3 text-[12px]">
                <div className="flex flex-wrap items-center gap-3">
                  <span className="font-mono text-forensics-muted">{boot.timestamp}</span>
                  <span className="rounded-none bg-forensics-panel-strong px-2 py-0.5 text-[10px]">
                    {boot.bootType}
                  </span>
                  {boot.eventId ? (
                    <span className="rounded-none bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px]">
                      EventID {boot.eventId}
                    </span>
                  ) : null}
                  <span className="text-forensics-muted-lighter">{boot.source}</span>
                </div>
                {boot.note ? <div className="mt-2 text-[11px] text-forensics-muted">{boot.note}</div> : null}
                <div className="mt-2 text-[11px] text-forensics-muted">
                  {formatProvenanceSummary(boot.provenance)}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine text={t('analysisPanels.systemInfo.bootHistoryEmpty')} />
        )}
      </section>
    </div>
  );
}

export function AnalysisHeader({
  loading,
  hasCase,
  extractionPending,
  onRefresh,
  onRunExtraction,
  selectedDataSourceId,
}: {
  loading: boolean;
  hasCase: boolean;
  extractionPending: boolean;
  onRefresh: () => void;
  onRunExtraction: () => void;
  selectedDataSourceId?: string;
}) {
  const { t } = useTranslation();
  return (
    <ContextBar align="end">
      <div className="flex flex-wrap items-center justify-end gap-x-6 gap-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={onRefresh}
            disabled={!hasCase || loading || !selectedDataSourceId}
            className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"
          >
            <RefreshCw size={14} className={loading ? 'opacity-70' : ''} />
            {t('analysis.header.refresh')}
          </Button>
          <Button
            type="button"
            onClick={onRunExtraction}
            disabled={!hasCase || extractionPending || !selectedDataSourceId}
            className="h-8 rounded-none border border-forensics-text bg-forensics-text px-3 text-[12px] text-white hover:bg-forensics-text-secondary"
          >
            {extractionPending ? <RefreshCw size={14} className="opacity-70" /> : <Shield size={14} />}
            {extractionPending ? t('analysis.header.running') : t('analysis.header.run')}
          </Button>
        </div>
      </div>
    </ContextBar>
  );
}

export function AnalysisEmptyState() {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <BrandEmptyState
        title={t('analysisPanels.empty.noCaseTitle')}
        description={t('analysisPanels.empty.noCaseDescription')}
        className="max-w-md"
      />
    </div>
  );
}

export function AnalysisErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mb-4 flex items-center justify-between gap-3 rounded-none border border-forensics-error-border bg-forensics-error-bg p-3 text-[12px] text-forensics-error-text">
      <span>{message}</span>
      <Button
        type="button"
        variant="outline"
        onClick={onRetry}
        className="h-7 shrink-0 rounded-none border-forensics-error-border bg-forensics-surface px-3 text-[12px] text-forensics-error-text hover:bg-forensics-error-bg"
      >
        {t('viewer.error.retry')}
      </Button>
    </div>
  );
}

export function AnalysisLoadingPanel({ text }: { text: string }) {
  return (
    <div className="flex h-64 items-center justify-center text-forensics-muted-lighter">
      <RefreshCw size={24} className="mr-2 opacity-70" />
      {text}
    </div>
  );
}
