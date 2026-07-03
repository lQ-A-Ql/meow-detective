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

/** Linux artifact summary — mirrors Rust `LinuxArtifactSummaryDto`. */
export interface LinuxArtifactSummary {
  status: AnalysisParseStatus;
  journalCount: number;
  loginCount: number;
  bashCommandCount: number;
  aptEventCount: number;
  cronJobCount: number;
  sudoEventCount: number;
  totalCount: number;
  journalEntries: LinuxJournalEntry[];
  loginRecords: LinuxLoginRecord[];
  bashCommands: LinuxBashCommand[];
  aptEvents: LinuxAptEvent[];
  cronJobs: LinuxCronJob[];
  sudoEvents: LinuxSudoEvent[];
  generatedAt: string;
  warnings: string[];
}
