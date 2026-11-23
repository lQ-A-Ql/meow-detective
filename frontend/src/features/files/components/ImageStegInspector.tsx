import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { Button } from '@/app/components/ui/button';
import { InspectorSection, InspectorSectionTitle } from '@/components/layout/InspectorPane';
import type { ImageIssueSeverity, ImageStegAssessment, ImageStegBand, ImageStegReport } from '@/types/models';
import type { ImageStegModel } from '@/features/files/hooks/use-image-steg';
import { formatStegRatio, formatStegScore } from '@/features/files/image-steg-format';

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

function AssessmentBlock({ assessment }: { assessment: ImageStegAssessment }) {
  const { t } = useTranslation();
  const methods = assessment.methods ?? [];
  return (
    <div className="space-y-1">
      <div className="grid grid-cols-[92px_1fr] gap-1 font-mono text-[10px]">
        <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.score')}</span>
        <span className={bandClass(assessment.band)}>
          {formatStegScore(assessment.score)} · {t(`fileBrowser.inspector.imageSteg.bands.${assessment.band}`)}
        </span>
        <span className="text-forensics-muted-light">{t('fileBrowser.inspector.imageSteg.methodsUsed')}</span>
        <span className="text-forensics-text-secondary">{assessment.methodsUsed}</span>
      </div>
      {methods.length > 0 ? (
        <div className="space-y-0.5">
          {methods.map((method) => (
            <div
              key={method.method}
              className="grid grid-cols-[64px_1fr] gap-1 font-mono text-[10px] text-forensics-text-secondary"
            >
              <span className="text-forensics-muted-light">
                {t(`fileBrowser.inspector.imageSteg.methods.${method.method}`)}
              </span>
              <span>
                {formatStegRatio(method.estimate)} · {formatStegScore(method.score)} · {method.samples}
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
  const planes = report.planes ?? [];
  const blocks = report.blocks ?? [];
  const warnings = report.warnings ?? [];

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
          <InspectorSectionTitle compact>{t('fileBrowser.inspector.imageSteg.global')}</InspectorSectionTitle>
          <AssessmentBlock assessment={report.global} />
        </div>
      ) : null}

      {planes.length > 0 ? (
        <div>
          <InspectorSectionTitle compact>{t('fileBrowser.inspector.imageSteg.planes')}</InspectorSectionTitle>
          <div className="max-h-40 space-y-1 overflow-auto">
            {planes.map((plane) => (
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
                    ? ` · ${formatStegScore(plane.assessment.score)} · ${t(
                        `fileBrowser.inspector.imageSteg.bands.${plane.assessment.band}`,
                      )}`
                    : ''}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {blocks.length > 0 ? (
        <div>
          <InspectorSectionTitle compact>{t('fileBrowser.inspector.imageSteg.hotspots')}</InspectorSectionTitle>
          <div className="max-h-32 space-y-0.5 overflow-auto">
            {blocks.map((block) => (
              <div
                key={`${block.plane}-${block.x}-${block.y}`}
                className={`grid grid-cols-[48px_1fr] gap-1 font-mono text-[10px] ${bandClass(block.band)}`}
              >
                <span>{block.plane}</span>
                <span>
                  {block.x},{block.y} · {block.width}×{block.height} · {formatStegScore(block.score)}
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

      {warnings.length > 0 ? (
        <div>
          <InspectorSectionTitle compact>{t('fileBrowser.inspector.imageSteg.warnings')}</InspectorSectionTitle>
          <div className="space-y-1">
            {warnings.map((warning, index) => (
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
  const runLabel = model.running
    ? t('fileBrowser.inspector.imageSteg.running')
    : t('fileBrowser.inspector.imageSteg.run');

  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.imageSteg')}>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Tooltip content={runLabel}>
            <Button
              aria-label={runLabel}
              type="button"
              size="xs"
              variant="forensicsOutline"
              onClick={model.run}
              disabled={model.running}
            >
              <ActionIcon name="play" />
            </Button>
          </Tooltip>
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
