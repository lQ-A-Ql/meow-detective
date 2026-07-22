import type { AnalysisParseStatus } from './analysis';

/** EVTX event category — mirrors Rust `EvtxEventCategoryDto`. */
export type EvtxEventCategory =
  | 'bootShutdown'
  | 'logonLogoff'
  | 'privilegeEscalation'
  | 'processExecution'
  | 'accountManagement'
  | 'scheduledTask'
  | 'applicationCrash'
  | 'softwareInstallation'
  | 'other';

export type EvtxEventView = 'boot' | 'logon' | 'process' | 'account' | 'application';

/** Boot/shutdown event from System.evtx. */
export interface EvtxBootEvent {
  timestamp: string;
  eventId: number;
  recordId?: number;
  provider?: string;
  kind: string;
  sourcePath: string;
  note: string;
  details?: Record<string, string>;
}

/** Security audit event from Security.evtx. */
export interface EvtxSecurityEvent {
  timestamp: string;
  eventId: number;
  recordId?: number;
  provider?: string;
  kind: string;
  sourcePath: string;
  targetUser?: string;
  subjectUser?: string;
  logonType?: string;
  ipAddress?: string;
  workstation?: string;
  failureReason?: string;
  processName?: string;
  parentProcessName?: string;
  taskName?: string;
  privilegeList?: string;
  memberName?: string;
  details?: Record<string, string>;
}

/** Application event from Application.evtx. */
export interface EvtxApplicationEvent {
  timestamp: string;
  eventId: number;
  recordId?: number;
  provider?: string;
  kind: string;
  sourcePath: string;
  application?: string;
  faultModule?: string;
  productName?: string;
  manufacturer?: string;
  details?: Record<string, string>;
}

/** Unified EVTX event log summary. */
export interface EvtxEventSummary {
  status: AnalysisParseStatus;
  pageTotal: number;
  bootShutdownCount: number;
  logonLogoffCount: number;
  privilegeEscalationCount: number;
  processExecutionCount: number;
  accountManagementCount: number;
  scheduledTaskCount: number;
  applicationCrashCount: number;
  softwareInstallationCount: number;
  otherCount: number;
  totalCount: number;
  bootEvents: EvtxBootEvent[];
  securityEvents: EvtxSecurityEvent[];
  applicationEvents: EvtxApplicationEvent[];
  warnings: string[];
  generatedAt: string;
}
