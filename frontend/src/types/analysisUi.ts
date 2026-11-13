export type AnalysisExtractionProgressState = 'idle' | 'running' | 'success' | 'partial' | 'failed';

export interface AnalysisExtractionProgressInfo {
  label: string;
  status: AnalysisExtractionProgressState;
  scannedCount: number;
  artifactCount: number;
  timelineEventCount: number;
  warnings: string[];
  error?: string;
  totalCandidateCount?: number;
  processedCandidateCount?: number;
  structuredCandidateCount?: number;
  unsupportedCandidateCount?: number;
  textFallbackCandidateCount?: number;
  warningCandidateCount?: number;
  checkpointHitCount?: number;
  phase?: string;
  currentPath?: string;
  detail?: string;
}

export type AnalysisTabKey =
  | 'system'
  | 'evidence'
  | 'registry'
  | 'browser'
  | 'email'
  | 'eventlogs'
  | 'files'
  | 'report'
  | 'deletedRecovery'
  | 'plugin';

export type LinuxAnalysisTabKey =
  | 'overview'
  | 'journal'
  | 'login'
  | 'commands'
  | 'packages'
  | 'cron'
  | 'sudo'
  | 'systemConfig'
  | 'webServices'
  | 'mysqlServices'
  | 'services'
  | 'docker'
  | 'deletedRecovery'
  | 'plugin';

export type AndroidAnalysisTabKey = 'device' | 'packages';

export type ExtractionCategory =
  | 'Registry'
  | 'BrowserHistory'
  | 'Email'
  | 'EventLogs'
  | 'ProgramExecution'
  | 'UserActivity'
  | 'RecycleBin'
  | 'Thumbnails'
  | 'ResourceUsage'
  | 'PluginArtifacts'
  | 'LinuxArtifacts'
  | 'LinuxJournal'
  | 'LinuxLogin'
  | 'LinuxCommands'
  | 'LinuxPackages'
  | 'LinuxCron'
  | 'LinuxSudo'
  | 'LinuxSystemConfig'
  | 'LinuxWebServices'
  | 'LinuxMysqlServices';

export const ANALYSIS_EXTRACTION_CATEGORIES = [
  'Registry',
  'BrowserHistory',
  'Email',
  'EventLogs',
  'ProgramExecution',
  'UserActivity',
  'RecycleBin',
  'Thumbnails',
  'ResourceUsage',
  'PluginArtifacts',
  'LinuxArtifacts',
  'LinuxJournal',
  'LinuxLogin',
  'LinuxCommands',
  'LinuxPackages',
  'LinuxCron',
  'LinuxSudo',
  'LinuxSystemConfig',
  'LinuxWebServices',
  'LinuxMysqlServices',
] as const satisfies readonly ExtractionCategory[];

const EXTRACTION_CATEGORY_SET = new Set<string>(ANALYSIS_EXTRACTION_CATEGORIES);

export function isExtractionCategory(value: string): value is ExtractionCategory {
  return EXTRACTION_CATEGORY_SET.has(value);
}
