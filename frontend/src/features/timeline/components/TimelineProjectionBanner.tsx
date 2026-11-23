import { CircleAlert, LoaderCircle, RefreshCw } from '@/components/icons';
import { Button } from '@/app/components/ui/button';
import { StatusNotice } from '@/app/components/ui/status-notice';
import { useTranslation } from 'react-i18next';
import { formatBytes } from '@/lib/format-bytes';
import { errorMessage } from '@/lib/errors';
import type {
  CapabilityProgress,
  CapabilityState,
  TimelineProjectionSourceStatus,
  TimelineProjectionState,
} from '@/types/models';

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
  /**
   * Optional capability row from the matrix. When present and non-ready it drives
   * the strip (running/queued/partial/deferred/failed); omitted keeps the legacy
   * `state` behavior untouched.
   */
  capabilityState?: CapabilityState;
  capabilityProgress?: CapabilityProgress;
  capabilityDetail?: string;
  capabilityError?: string;
}

function detailSource(
  sources: TimelineProjectionSourceStatus[],
  state: TimelineProjectionState,
): TimelineProjectionSourceStatus | undefined {
  return sources.find((source) => source.state === state) ?? sources[0];
}

function formatProgress(progress?: CapabilityProgress): string | undefined {
  if (!progress) return undefined;
  const phase = progress.phase ? ` · ${progress.phase}` : '';
  return progress.total !== undefined ? `${progress.done}/${progress.total}${phase}` : `${progress.done}${phase}`;
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
  capabilityState,
  capabilityProgress,
  capabilityDetail,
  capabilityError,
}: TimelineProjectionBannerProps) {
  const { t } = useTranslation();

  const capabilityActive =
    capabilityState !== undefined && capabilityState !== 'ready' && capabilityState !== 'notApplicable';

  if (!capabilityActive && (!state || state === 'complete')) {
    return null;
  }

  // --- Capability-matrix branch (running/queued/partial/deferred/failed) ---
  if (capabilityActive) {
    const running = capabilityState === 'running' || capabilityState === 'queued';
    const failed = capabilityState === 'failed';
    const deferred = capabilityState === 'deferred';
    const tone = failed ? 'error' : deferred || running ? 'warning' : 'neutral';
    const progressText = formatProgress(capabilityProgress);

    const title = failed
      ? t('timeline.projection.capabilityFailedTitle')
      : deferred
        ? t('timeline.projection.deferredTitle')
        : running
          ? t('timeline.projection.capabilityRunningTitle')
          : t('timeline.projection.capabilityPartialTitle');
    const description = failed
      ? t('timeline.projection.capabilityFailedDescription')
      : deferred
        ? t('timeline.projection.deferredDescription')
        : running
          ? t('timeline.projection.capabilityRunningDescription')
          : t('timeline.projection.capabilityPartialDescription');

    return (
      <StatusNotice tone={tone} data-testid="timeline-projection-banner">
        <div className="flex items-center gap-2">
          {rebuilding || running ? (
            <LoaderCircle size={14} className="shrink-0 animate-spin" />
          ) : (
            <CircleAlert size={14} className="shrink-0" />
          )}
          <div className="min-w-0 flex-1">
            <div className="text-[12px] font-medium">{title}</div>
            <div className="text-[11px] opacity-80">{description}</div>
            {progressText ? <div className="mt-1 font-mono text-[11px]">{progressText}</div> : null}
          </div>
          {running ? (
            <Button type="button" shape="pill" variant="forensicsOutline" size="xs" disabled>
              <LoaderCircle size={12} className="animate-spin" />
              {t('timeline.projection.buildingAction')}
            </Button>
          ) : (
            <Button
              type="button"
              shape="pill"
              variant="forensicsOutline"
              size="xs"
              disabled={rebuilding}
              onClick={() => onRebuild(undefined)}
            >
              {rebuilding ? <LoaderCircle size={12} className="animate-spin" /> : <RefreshCw size={12} />}
              {rebuilding
                ? t('timeline.projection.rebuilding')
                : failed
                  ? t('timeline.projection.capabilityRetry')
                  : t('timeline.projection.rebuild')}
            </Button>
          )}
        </div>
        {(capabilityError ?? capabilityDetail) ? (
          <div className="mt-1 text-[11px]">
            <span className="opacity-70">{t('timeline.projection.reasonLabel')}: </span>
            {capabilityError ?? capabilityDetail}
          </div>
        ) : null}
        {rebuildError ? (
          <div className="mt-1 text-[11px] font-medium">
            {t('timeline.projection.rebuildFailed', { message: errorMessage(rebuildError) })}
          </div>
        ) : null}
      </StatusNotice>
    );
  }

  // --- Legacy projection-status branch (complete/building/deferred) ---
  const detail = detailSource(sources, state!);
  const tone = state === 'building' ? 'warning' : 'error';

  return (
    <StatusNotice tone={tone} data-testid="timeline-projection-banner">
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
          <Button type="button" shape="pill" variant="forensicsOutline" size="xs" disabled>
            <LoaderCircle size={12} className="animate-spin" />
            {t('timeline.projection.buildingAction')}
          </Button>
        ) : (
          <Button
            type="button"
            shape="pill"
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
    </StatusNotice>
  );
}
