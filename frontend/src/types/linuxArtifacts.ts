import type { AnalysisParseStatus } from './analysis';

/** systemd journal entry — mirrors Rust `LinuxJournalEntryDto`. */
export interface LinuxJournalEntry {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  timestamp?: string;
  message?: string;
  executable?: string;
  systemdUnit?: string;
  hostname?: string;
  syslogIdentifier?: string;
  pid?: number;
  priority?: number;
}

/** wtmp/btmp login record — mirrors Rust `LinuxLoginRecordDto`. */
export interface LinuxLoginRecord {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  user: string;
  terminal: string;
  host: string;
  pid: number;
  recordType: number;
  loginTime?: string;
  logoutTime?: string;
}

/** bash_history command — mirrors Rust `LinuxBashCommandDto`. */
export interface LinuxBashCommand {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  command: string;
  lineNumber: number;
  timestamp?: string;
}

/** apt/dpkg package event — mirrors Rust `LinuxAptEventDto`. */
export interface LinuxAptEvent {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  action: string;
  package: string;
  version: string;
  timestamp?: string;
}

/** crontab entry — mirrors Rust `LinuxCronJobDto`. */
export interface LinuxCronJob {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  schedule: string;
  command: string;
  user?: string;
  sourceFile: string;
}

/** sudo/auth.log event — mirrors Rust `LinuxSudoEventDto`. */
export interface LinuxSudoEvent {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  user: string;
  targetUser?: string;
  command: string;
  workingDirectory?: string;
  terminal?: string;
  success: boolean;
  timestamp?: string;
}

/** Linux system/config/persistence record — mirrors Rust `LinuxSystemConfigDto`. */
export interface LinuxSystemConfig {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  configKind: string;
  line: string;
  lineNumber: number;
  key?: string;
  value?: string;
  username?: string;
  uid?: number;
  gid?: number;
  home?: string;
  shell?: string;
}

/** Linux nginx/apache site record — mirrors Rust `LinuxWebSiteDto`. */
export interface LinuxWebSite {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  serverKind: string;
  siteName: string;
  hostnames: string[];
  listen: string[];
  documentRoots: string[];
  accessLogs: string[];
  errorLogs: string[];
  lineNumber: number;
}

/** Linux nginx/apache access log entry — mirrors Rust `LinuxWebAccessLogDto`. */
export interface LinuxWebAccessLog {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  clientIp: string;
  timestamp?: string;
  method: string;
  uri: string;
  protocol: string;
  status: number;
  responseBytes?: number;
  referer?: string;
  userAgent?: string;
  lineNumber: number;
}

/** Linux nginx/apache error log entry — mirrors Rust `LinuxWebErrorLogDto`. */
export interface LinuxWebErrorLog {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  timestamp?: string;
  severity?: string;
  message: string;
  lineNumber: number;
}

/** Linux Web service finding — mirrors Rust `LinuxWebFindingDto`. */
export interface LinuxWebFinding {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  findingKind: string;
  severity: string;
  confidence: number;
  evidence: string;
  clientIp?: string;
  uri?: string;
  timestamp?: string;
  lineNumber: number;
}

/** Linux MySQL/MariaDB config record - mirrors Rust `LinuxMysqlConfigDto`. */
export interface LinuxMysqlConfig {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  section?: string;
  key: string;
  value: string;
  lineNumber: number;
}

/** Linux MySQL/MariaDB log entry - mirrors Rust `LinuxMysqlLogEntryDto`. */
export interface LinuxMysqlLogEntry {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  timestamp?: string;
  severity?: string;
  threadId?: string;
  message: string;
  lineNumber: number;
}

/** Linux MySQL/MariaDB finding - mirrors Rust `LinuxMysqlFindingDto`. */
export interface LinuxMysqlFinding {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  findingKind: string;
  severity: string;
  confidence: number;
  evidence: string;
  lineNumber: number;
}

/** Linux artifact summary — mirrors Rust `LinuxArtifactSummaryDto`. */
export interface LinuxArtifactSummary {
  status: AnalysisParseStatus;
  journalCount: number;
  loginCount: number;
  bashCommandCount: number;
  aptEventCount: number;
  cronJobCount: number;
  sudoEventCount: number;
  systemConfigCount: number;
  webSiteCount: number;
  webAccessLogCount: number;
  webErrorLogCount: number;
  webFindingCount: number;
  mysqlConfigCount: number;
  mysqlLogCount: number;
  mysqlFindingCount: number;
  totalCount: number;
  truncated: boolean;
  coverageRatio: number;
  journalEntries: LinuxJournalEntry[];
  loginRecords: LinuxLoginRecord[];
  bashCommands: LinuxBashCommand[];
  aptEvents: LinuxAptEvent[];
  cronJobs: LinuxCronJob[];
  sudoEvents: LinuxSudoEvent[];
  systemConfigs: LinuxSystemConfig[];
  webSites: LinuxWebSite[];
  webAccessLogs: LinuxWebAccessLog[];
  webErrorLogs: LinuxWebErrorLog[];
  webFindings: LinuxWebFinding[];
  mysqlConfigs: LinuxMysqlConfig[];
  mysqlLogs: LinuxMysqlLogEntry[];
  mysqlFindings: LinuxMysqlFinding[];
  generatedAt: string;
  warnings: string[];
}
