import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { Button } from '@/app/components/ui/button';
import { InspectorSection } from '@/components/layout/InspectorPane';
import type { ImageIssueSeverity, ImageStegAssessment, ImageStegBand, ImageStegReport } from '@/types/models';
import type { ImageStegModel } from '@/features/files/hooks/use-image-steg';

function formatPercent(value?: number) {
  if (value === undefined || !Number.isFinite(value)) return '-';
  return `${(value * 100).toFixed(1)}%`;
}

function bandClass(band: ImageStegBand) {
  if (band === 'high') return 'text-forensics-error-text';
  if (band === 'medium') return 'text-forensics-warning-text';
  if (band === 'low') return 'text-forensics-text-secondary';
  return 'text-forensics-muted-light';
}

function severityClass(severity: ImageIssueSeverity) {
  if (severity === 'error') return 'text-forensics-error-text';
  if (severity === 'warning') return 'text-forensics-warning-text';
  return 'text-forensics-muted-light';
}

function SectionLabel({ label }: { label: string }) {
  return (
    <div className="mb-1 border-b border-forensics-border text-[10px] uppercase tracking-wide text-forensics-muted-light">
      {label}
    </div>
  );
}

function AssessmentBlock({ assessment }: { assessment: ImageStegAssessment }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-1">
      <div className="grid grid-cols-[92px_1fr] gap-1 font-mono text-[10px]">
        <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.score')}</span>
        <span className={bandClass(assessment.band)}>
          {formatPercent(assessment.score)} · {t(`fileBrowser.inspector.imageSteg.bands.${assessment.band}`)}
        </span>
        <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.methodsUsed')}</span>
        <span className="text-forensics-text-secondary">{assessment.methodsUsed}</span>
      </div>
      {assessment.methods.length > 0 ? (
        <div className="space-y-0.5">
          {assessment.methods.map((method) => (
            <div
              key={method.method}
              className="grid grid-cols-[64px_1fr] gap-1 font-mono text-[10px] text-forensics-text-secondary"
            >
              <span className="text-forensics-muted-light">
                {t(`fileBrowser.inspector.imageSteg.methods.${method.method}`)}
              </span>
              <span>
                {formatPercent(method.estimate)} · {formatPercent(method.score)} · {method.samples}
              </span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function StegReportView({ report }: { report: ImageStegReport }) {
  const { t } = useTranslation();
  const dimensions =
    report.width !== undefined && report.height !== undefined ? `${report.width} × ${report.height}` : undefined;

  return (
    <div className="space-y-2" data-testid="image-steg-report">
      <div className="grid grid-cols-[92px_1fr] gap-1 font-mono text-[10px]">
        <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.status')}</span>
        <span className="text-forensics-text-secondary">
          {t(`fileBrowser.inspector.imageSteg.statuses.${report.status}`)}
        </span>
        {report.format ? (
          <>
            <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.format')}</span>
            <span className="text-forensics-text-secondary">{report.format}</span>
          </>
        ) : null}
        {dimensions ? (
          <>
            <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.dimensions')}</span>
            <span className="text-forensics-text-secondary">{dimensions}</span>
          </>
        ) : null}
        {report.reason ? (
          <>
            <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.reason')}</span>
            <span className="break-all text-forensics-warning-text">
              {t(`fileBrowser.inspector.imageSteg.reasons.${report.reason}`, { defaultValue: report.reason })}
            </span>
          </>
        ) : null}
        {report.dominantPlane ? (
          <>
            <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.dominantPlane')}</span>
            <span className="text-forensics-text-secondary">{report.dominantPlane}</span>
          </>
        ) : null}
      </div>

      {report.status === 'completed' && report.global ? (
        <div>
          <SectionLabel label={t('fileBrowser.inspector.imageSteg.global')} />
          <AssessmentBlock assessment={report.global} />
        </div>
      ) : null}

      {report.planes.length > 0 ? (
        <div>
          <SectionLabel label={t('fileBrowser.inspector.imageSteg.planes')} />
          <div className="max-h-40 space-y-1 overflow-auto">
            {report.planes.map((plane) => (
              <div
                key={plane.plane}
                className="grid grid-cols-[48px_1fr] gap-1 font-mono text-[10px] text-forensics-text-secondary"
              >
                <span className="text-forensics-muted-light">{plane.plane}</span>
                <span>
                  {t('fileBrowser.inspector.imageSteg.planeSamples', {
                    samples: plane.samples,
                    unique: plane.uniqueValues,
                  })}
                  {plane.assessment
                    ? ` · ${formatPercent(plane.assessment.score)} · ${t(
                        `fileBrowser.inspector.imageSteg.bands.${plane.assessment.band}`,
                      )}`
                    : ''}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {report.blocks.length > 0 ? (
        <div>
          <SectionLabel label={t('fileBrowser.inspector.imageSteg.hotspots')} />
          <div className="max-h-32 space-y-0.5 overflow-auto">
            {report.blocks.map((block) => (
              <div
                key={`${block.plane}-${block.x}-${block.y}`}
                className={`grid grid-cols-[48px_1fr] gap-1 font-mono text-[10px] ${bandClass(block.band)}`}
              >
                <span>{block.plane}</span>
                <span>
                  {block.x},{block.y} · {block.width}×{block.height} · {formatPercent(block.score)}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {report.blocksTotal > 0 ? (
        <div className="font-mono text-[10px] text-forensics-muted-light">
          {t('fileBrowser.inspector.imageSteg.blockSummary', {
            scored: report.blocksScored,
            total: report.blocksTotal,
          })}
        </div>
      ) : null}

      {report.warnings.length > 0 ? (
        <div>
          <SectionLabel label={t('fileBrowser.inspector.imageSteg.warnings')} />
          <div className="space-y-1">
            {report.warnings.map((warning, index) => (
              <div
                key={`${warning.code}-${index}`}
                className={`space-y-0.5 font-mono text-[10px] ${severityClass(warning.severity)}`}
                data-severity={warning.severity}
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="uppercase">{warning.severity}</span>
                  <span>{warning.code}</span>
                </div>
                <div className="break-all">
                  {t(`fileBrowser.inspector.imageSteg.warningCodes.${warning.code}`, {
                    defaultValue: warning.message,
                  })}
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

export function ImageStegInspector({ model }: { model: ImageStegModel }) {
  const { t } = useTranslation();
  const idle = !model.report && !model.running && !model.errorText;

  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.imageSteg')}>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Button
            type="button"
            shape="pill"
            size="xs"
            variant="forensicsOutline"
            onClick={model.run}
            disabled={model.running}
          >
            {model.running ? t('fileBrowser.inspector.imageSteg.running') : t('fileBrowser.inspector.imageSteg.run')}
          </Button>
        </div>
        {idle ? (
          <div className="space-y-1 text-[10px] text-forensics-muted">
            <div>{t('fileBrowser.inspector.imageSteg.description')}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.sessionOnly')}</div>
          </div>
        ) : null}
        {!model.running && model.errorText ? (
          <div className="space-y-2">
            <div className="text-[11px] text-forensics-error-text">{model.errorText}</div>
            <Tooltip content={t('fileBrowser.inspector.imageSteg.retry')}>
              <Button
                aria-label={t('fileBrowser.inspector.imageSteg.retry')}
                type="button"
                size="xs"
                variant="forensicsOutline"
                onClick={model.run}
              >
                <ActionIcon name="refresh" />
              </Button>
            </Tooltip>
          </div>
        ) : null}
        {!model.running && model.report ? <StegReportView report={model.report} /> : null}
      </div>
    </InspectorSection>
  );
}
