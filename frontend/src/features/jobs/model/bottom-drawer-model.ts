import type { TFunction } from 'i18next';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
import {
  deriveEvidenceHashStatus,
  getImportPhaseLabel,
  getImportPhaseStateLabel,
  type EvidenceHashStatus,
  type ImportSignalSnapshot,
} from '@/features/jobs/import-event-state';
import type {
  ApiErrorDto,
  DataSourceSummary,
  ImportPhaseProgress,
  JobSnapshot,
  TraceItem,
  WarningItem,
} from '@/types/models';

export type DrawerIssueSeverity = 'error' | 'warning';

export interface DrawerIssueMeta {
  label: string;
  value: string;
}

export interface DrawerIssue {
  id: string;
  severity: DrawerIssueSeverity;
  title: string;
  detail: string;
  meta: DrawerIssueMeta[];
  details?: string;
  suggestion?: string;
}

export interface BottomDrawerModel {
  drawerOpen: boolean;
  toggleDrawer: () => void;
  runningJobs: JobSnapshot[];
  completedJobs: JobSnapshot[];
  failedJobs: JobSnapshot[];
  warningJobs: JobSnapshot[];
  cancellingJobs: JobSnapshot[];
  cancelledJobs: JobSnapshot[];
  partialJobCount: number;
  jobSkippedCount: number;
  errorIssues: DrawerIssue[];
  warningIssues: DrawerIssue[];
  evidenceHashStatus?: EvidenceHashStatus;
  importSignals: ImportSignalSnapshot;
  trace: TraceItem[];
  headline: string;
  recentScope: string;
}

interface BottomDrawerModelInput {
  drawerOpen: boolean;
  toggleDrawer: () => void;
  jobs?: JobSnapshot[];
  jobsError?: unknown;
  warnings?: WarningItem[];
  warningsError?: unknown;
  dataSources?: DataSourceSummary[];
  dataSourcesError?: unknown;
  trace?: TraceItem[];
  traceError?: unknown;
  importSignals: ImportSignalSnapshot;
  t: TFunction;
}

export function buildBottomDrawerModel(input: BottomDrawerModelInput): BottomDrawerModel {
  const jobs = input.jobs ?? [];
  const dataSources = input.dataSources ?? [];
  const runningJobs = jobs.filter((job) => job.status === 'running');
  const completedJobs = jobs.filter((job) => job.status === 'completed');
  const failedJobs = jobs.filter((job) => job.status === 'failed');
  const warningJobs = jobs.filter((job) => job.status === 'warning');
  const cancellingJobs = jobs.filter((job) => job.status === 'cancelling');
  const cancelledJobs = jobs.filter((job) => job.status === 'cancelled');
  const queryIssues = [
    buildQueryIssue('jobs', input.t('bottomDrawer.issues.sources.jobs'), input.jobsError, input.t),
    buildQueryIssue('warnings', input.t('bottomDrawer.issues.sources.warnings'), input.warningsError, input.t),
    buildQueryIssue(
      'dataSources',
      input.t('bottomDrawer.issues.sources.dataSources'),
      input.dataSourcesError,
      input.t,
    ),
    buildQueryIssue('trace', input.t('bottomDrawer.issues.sources.trace'), input.traceError, input.t),
  ].filter((issue): issue is DrawerIssue => Boolean(issue));
  const processingIssues = buildDataSourceProcessingIssues(dataSources, input.t);
  const importPhaseIssue = buildImportPhaseIssue(input.importSignals.latestPhase, input.t);
  const errorIssues = [
    ...queryIssues,
    ...failedJobs.map((job) => buildFailedJobIssue(job, input.t)),
    ...processingIssues.filter((issue) => issue.severity === 'error'),
    ...(importPhaseIssue ? [importPhaseIssue] : []),
  ];
  const warningIssues = [
    ...buildWarningIssues(input.warnings ?? []),
    ...buildJobWarningIssues(jobs, input.t),
    ...processingIssues.filter((issue) => issue.severity === 'warning'),
  ];
  const typedHeadline = input.importSignals.latestCancellation
    ? `${
        input.importSignals.latestCancellation.safeToClose
          ? input.t('bottomDrawer.labels.safeToClose')
          : input.importSignals.latestCancellation.state
      } · ${input.importSignals.latestCancellation.detail}`
    : input.importSignals.latestPhase
      ? `${getImportPhaseLabel(input.importSignals.latestPhase.phase)} ${input.importSignals.latestPhase.percent}% · ${input.importSignals.latestPhase.detail}`
      : undefined;
  const recentScope =
    runningJobs[0]?.scope ||
    warningJobs[0]?.scope ||
    cancellingJobs[0]?.scope ||
    failedJobs[0]?.scope ||
    completedJobs[0]?.scope ||
    input.t('bottomDrawer.status.idle');

  return {
    drawerOpen: input.drawerOpen,
    toggleDrawer: input.toggleDrawer,
    runningJobs,
    completedJobs,
    failedJobs,
    warningJobs,
    cancellingJobs,
    cancelledJobs,
    partialJobCount: jobs.filter((job) => job.partial).length,
    jobSkippedCount: jobs.reduce((sum, job) => sum + job.skippedCount, 0),
    errorIssues,
    warningIssues,
    evidenceHashStatus: deriveEvidenceHashStatus(input.importSignals.partialResults, dataSources),
    importSignals: input.importSignals,
    trace: input.trace ?? [],
    headline:
      typedHeadline ||
      runningJobs[0]?.detail ||
      warningJobs[0]?.detail ||
      failedJobs[0]?.detail ||
      cancellingJobs[0]?.detail ||
      completedJobs[0]?.detail ||
      input.t('bottomDrawer.headline.waiting'),
    recentScope,
  };
}

function buildQueryIssue(source: string, title: string, error: unknown, t: TFunction): DrawerIssue | undefined {
  if (!error) return undefined;
  const apiError = isApiErrorDto(error) ? error : undefined;
  return {
    id: `query-${source}`,
    severity: 'error',
    title,
    detail: errorMessage(error),
    meta: buildApiErrorMeta(apiError, source, t),
    details: formatErrorDetails(apiError?.details),
    suggestion: apiError?.suggestion,
  };
}

function buildApiErrorMeta(apiError: ApiErrorDto | undefined, source: string, t: TFunction): DrawerIssueMeta[] {
  const meta: DrawerIssueMeta[] = [{ label: t('bottomDrawer.issues.meta.source'), value: source }];
  if (!apiError) return meta;
  meta.push({ label: t('bottomDrawer.issues.meta.code'), value: apiError.code });
  if (apiError.category) {
    meta.push({ label: t('bottomDrawer.issues.meta.category'), value: apiError.category });
  }
  if (apiError.recoverable !== undefined) {
    meta.push({
      label: t('bottomDrawer.issues.meta.recoverable'),
      value: apiError.recoverable
        ? t('bottomDrawer.issues.recoverable.yes')
        : t('bottomDrawer.issues.recoverable.no'),
    });
  }
  return meta;
}

function buildFailedJobIssue(job: JobSnapshot, t: TFunction): DrawerIssue {
  return {
    id: `job-${job.id}`,
    severity: 'error',
    title: job.name,
    detail: job.detail || t('bottomDrawer.jobs.failedFallback'),
    meta: jobIssueMeta(job, t),
  };
}

function buildImportPhaseIssue(phase: ImportPhaseProgress | undefined, t: TFunction): DrawerIssue | undefined {
  const failedCount = phase?.metrics.failed ?? 0;
  if (!phase || (phase.state !== 'failed' && failedCount === 0)) return undefined;
  return {
    id: `import-phase-${phase.jobId}-${phase.phase}`,
    severity: 'error',
    title: t('bottomDrawer.issues.importPhaseTitle'),
    detail: phase.detail || t('bottomDrawer.jobs.failedFallback'),
    meta: [
      { label: t('bottomDrawer.issues.meta.jobId'), value: phase.jobId },
      { label: t('bottomDrawer.issues.meta.phase'), value: getImportPhaseLabel(phase.phase) },
      { label: t('bottomDrawer.issues.meta.status'), value: getImportPhaseStateLabel(phase.state) },
      { label: t('bottomDrawer.issues.meta.progress'), value: `${phase.percent}%` },
      { label: t('bottomDrawer.labels.failed'), value: failedCount.toString() },
      { label: t('bottomDrawer.importSignals.processed'), value: phase.metrics.rowsProcessed.toString() },
    ],
  };
}

function buildWarningIssues(warnings: WarningItem[]): DrawerIssue[] {
  return warnings.map((warning) => ({
    id: `warning-${warning.id}`,
    severity: 'warning',
    title: warning.title,
    detail: warning.detail,
    meta: [],
  }));
}

function buildJobWarningIssues(jobs: JobSnapshot[], t: TFunction): DrawerIssue[] {
  return jobs
    .filter((job) => job.status !== 'failed')
    .filter((job) => job.status === 'warning' || job.partial || job.warningCount > 0 || job.skippedCount > 0)
    .map((job) => ({
      id: `job-warning-${job.id}`,
      severity: 'warning' as const,
      title: job.name,
      detail: job.detail || job.scope || t('bottomDrawer.issues.jobWarningFallback'),
      meta: jobIssueMeta(job, t),
    }));
}

function jobIssueMeta(job: JobSnapshot, t: TFunction): DrawerIssueMeta[] {
  return [
    { label: t('bottomDrawer.issues.meta.jobId'), value: job.id },
    { label: t('bottomDrawer.issues.meta.status'), value: job.status },
    { label: t('bottomDrawer.issues.meta.progress'), value: `${job.progress}%` },
    ...(job.scope ? [{ label: t('bottomDrawer.issues.meta.scope'), value: job.scope }] : []),
    { label: t('bottomDrawer.labels.failed'), value: job.failedCount.toString() },
    { label: t('bottomDrawer.labels.warnings'), value: job.warningCount.toString() },
    { label: t('bottomDrawer.labels.skipped'), value: job.skippedCount.toString() },
    ...(job.currentPartition
      ? [{ label: t('bottomDrawer.partitionProgress.title'), value: job.currentPartition }]
      : []),
  ];
}

function buildDataSourceProcessingIssues(dataSources: DataSourceSummary[], t: TFunction): DrawerIssue[] {
  return dataSources.flatMap((source) =>
    source.processing?.phases.flatMap((phase) => {
      const warningDetails = stringArray(phase.stats.warningDetails);
      const warningCount = numericValue(phase.stats.warningCount, warningDetails.length);
      const issues: DrawerIssue[] = [];
      if (phase.state === 'failed' || phase.state === 'deferred') {
        issues.push({
          id: `source-phase-${source.id}-${phase.phase}-${phase.state}`,
          severity: phase.state === 'failed' ? 'error' : 'warning',
          title: `${source.name} / ${phase.phase}`,
          detail: phase.lastError || `${phase.phase} ${phase.state}`,
          meta: [
            { label: t('bottomDrawer.issues.meta.source'), value: source.id },
            { label: t('bottomDrawer.issues.meta.phase'), value: phase.phase },
            { label: t('bottomDrawer.issues.meta.status'), value: phase.state },
          ],
        });
      }
      if (warningCount > 0) {
        const truncated = phase.stats.warningDetailsTruncated === true;
        issues.push({
          id: `source-phase-${source.id}-${phase.phase}-warnings`,
          severity: 'warning',
          title: `${source.name} / ${phase.phase}`,
          detail: warningDetails[0] || `${warningCount} processing warning(s)`,
          meta: [
            { label: t('bottomDrawer.issues.meta.source'), value: source.id },
            { label: t('bottomDrawer.issues.meta.phase'), value: phase.phase },
            { label: t('bottomDrawer.labels.warnings'), value: warningCount.toString() },
          ],
          details: warningDetails.length > 0
            ? `${warningDetails.join('\n')}${truncated ? '\n...' : ''}`
            : undefined,
        });
      }
      return issues;
    }) ?? [],
  );
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function numericValue(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function formatErrorDetails(details: unknown) {
  if (details === undefined || details === null) return undefined;
  if (typeof details === 'string') return details;
  try {
    return JSON.stringify(details, null, 2);
  } catch {
    return String(details);
  }
}
