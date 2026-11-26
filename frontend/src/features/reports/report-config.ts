import type { ReportFormat, ReportScope, ReportStatus } from '@/types/models';

export const DEFAULT_REPORT_SCOPE: ReportScope = {
  operatingSystem: true,
  hardware: true,
  browsers: true,
  email: true,
};

export const REPORT_FORMATS: readonly ReportFormat[] = ['html', 'csv', 'json', 'markdown'];
export const REPORT_SECTIONS = ['operatingSystem', 'hardware', 'browsers', 'email'] as const;

export const REPORT_ACTIONS: Record<ReportStatus, 'download' | 'retry' | null> = {
  running: null,
  completed: 'download',
  failed: 'retry',
  cancelled: 'retry',
};
