import { CircleAlert, LoaderCircle, RefreshCw } from '@/components/icons';
import { Button } from '@/app/components/ui/button';
import { useTranslation } from 'react-i18next';
import { formatBytes } from '@/lib/format-bytes';
import { errorMessage } from '@/lib/errors';
import type { TimelineProjectionSourceStatus, TimelineProjectionState } from '@/types/models';

interface TimelineProjectionBannerProps {
  /** Aggregate projection state from `get_timeline_projection_status`; undefined means not yet loaded. */
  state: TimelineProjectionState | undefined;
  sources: TimelineProjectionSourceStatus[];
  /** True while the rebuild mutation is in flight; blocks a second concurrent trigger. */
  rebuilding: boolean;
  /** Last rebuild mutation error; surfaced verbatim, never swallowed. */
  rebuildError: unknown;
  /** Invoked with no source id for a case-wide rebuild (empty request object). */
  onRebuild: (dataSourceId?: string) => void;
}

function detailSource(
  sources: TimelineProjectionSourceStatus[],
  state: TimelineProjectionState,
): TimelineProjectionSourceStatus | undefined {
  return sources.find((source) => source.state === state) ?? sources[0];
}

/**
 * Pure status strip for timeline projection. Renders nothing when the
 * projection is complete (or status has not loaded), so the existing page
 * stays untouched in the happy path.
 */
export function TimelineProjectionBanner({
  state,
  sources,
  rebuilding,
  rebuildError,
  onRebuild,
}: TimelineProjectionBannerProps) {
  const { t } = useTranslation();

  if (!state || state === 'complete') {
    return null;
  }

  const detail = detailSource(sources, state);
  const tone =
    state === 'building'
      ? 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text'
      : 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text';

  return (
    <div role="status" data-testid="timeline-projection-banner" className={`border-b px-4 py-3 ${tone}`}>
      <div className="flex items-center gap-2">
        {rebuilding ? (
          <LoaderCircle size={14} className="shrink-0 animate-spin" />
        ) : (
          <CircleAlert size={14} className="shrink-0" />
        )}
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-medium">
            {state === 'building' ? t('timeline.projection.buildingTitle') : t('timeline.projection.deferredTitle')}
          </div>
          <div className="text-[11px] opacity-80">
            {state === 'building'
              ? t('timeline.projection.buildingDescription')
              : t('timeline.projection.deferredDescription')}
          </div>
        </div>
        {state === 'building' ? (
          <Button type="button" variant="forensicsOutline" size="xs" disabled>
            <LoaderCircle size={12} className="animate-spin" />
            {t('timeline.projection.buildingAction')}
          </Button>
        ) : (
          <Button
            type="button"
            variant="forensicsOutline"
            size="xs"
            disabled={rebuilding}
            onClick={() => onRebuild(undefined)}
          >
            {rebuilding ? <LoaderCircle size={12} className="animate-spin" /> : <RefreshCw size={12} />}
            {rebuilding ? t('timeline.projection.rebuilding') : t('timeline.projection.rebuild')}
          </Button>
        )}
      </div>
      {detail?.reason ? (
        <div className="mt-1 text-[11px]">
          <span className="opacity-70">{t('timeline.projection.reasonLabel')}: </span>
          {detail.reason}
        </div>
      ) : null}
      {detail && (detail.requiredBytes !== undefined || detail.availableBytes !== undefined) ? (
        <div className="mt-1 font-mono text-[11px]">
          {detail.requiredBytes !== undefined
            ? t('timeline.projection.bytesRequired', { required: formatBytes(detail.requiredBytes) })
            : ''}
          {detail.requiredBytes !== undefined && detail.availableBytes !== undefined ? ' · ' : ''}
          {detail.availableBytes !== undefined
            ? t('timeline.projection.bytesAvailable', { available: formatBytes(detail.availableBytes) })
            : ''}
        </div>
      ) : null}
      {rebuildError ? (
        <div className="mt-1 text-[11px] font-medium">
          {t('timeline.projection.rebuildFailed', { message: errorMessage(rebuildError) })}
        </div>
      ) : null}
    </div>
  );
}
