import type { ComponentType } from 'react';
import { AppWindow,Database,FileClock,FileText,FileX2,Globe,Mail,Monitor,Server,Shield,Smartphone } from 'lucide-react';
import type { AndroidAnalysisTabKey,AnalysisTabKey,ExtractionCategory,LinuxAnalysisTabKey } from '../types';
export type SourceTreeNode = {
  labelKey: string;
  icon: ComponentType<{ size?: number | string; className?: string }>;
  category?: ExtractionCategory;
  windowsTab?: AnalysisTabKey;
  linuxTab?: LinuxAnalysisTabKey;
  androidTab?: AndroidAnalysisTabKey;
};

export const WINDOWS_NODES: SourceTreeNode[] = [
  { labelKey: 'analysis.tabs.system', icon: Monitor, windowsTab: 'system' },
  { labelKey: 'analysis.tabs.evidence', icon: Shield, windowsTab: 'evidence' },
  { labelKey: 'analysis.tabs.registry', icon: Database, category: 'Registry', windowsTab: 'registry' },
  { labelKey: 'analysis.tabs.browser', icon: Globe, category: 'BrowserHistory', windowsTab: 'browser' },
  { labelKey: 'analysis.tabs.email', icon: Mail, category: 'Email', windowsTab: 'email' },
  { labelKey: 'analysis.tabs.eventlogs', icon: FileClock, category: 'EventLogs', windowsTab: 'eventlogs' },
  { labelKey: 'analysis.tabs.files', icon: FileText, windowsTab: 'files' },
  { labelKey: 'analysis.tabs.report', icon: FileText, windowsTab: 'report' },
];

export const WINDOWS_DELETED_RECOVERY_NODE: SourceTreeNode = {
  labelKey: 'analysis.sidebar.deletedRecovery',
  icon: FileX2,
  windowsTab: 'deletedRecovery',
};

export const LINUX_NODES: SourceTreeNode[] = [
  { labelKey: 'analysis.sidebar.linuxOverview', icon: Server, category: 'LinuxArtifacts', linuxTab: 'overview' },
  { labelKey: 'analysis.sidebar.linuxJournal', icon: FileClock, category: 'LinuxJournal', linuxTab: 'journal' },
  { labelKey: 'analysis.sidebar.linuxLogin', icon: Monitor, category: 'LinuxLogin', linuxTab: 'login' },
  { labelKey: 'analysis.sidebar.linuxCommands', icon: FileText, category: 'LinuxCommands', linuxTab: 'commands' },
  { labelKey: 'analysis.sidebar.linuxPackages', icon: Database, category: 'LinuxPackages', linuxTab: 'packages' },
  { labelKey: 'analysis.sidebar.linuxCron', icon: FileClock, category: 'LinuxCron', linuxTab: 'cron' },
  { labelKey: 'analysis.sidebar.linuxSudo', icon: Shield, category: 'LinuxSudo', linuxTab: 'sudo' },
  { labelKey: 'analysis.sidebar.linuxSystemConfig', icon: Database, category: 'LinuxSystemConfig', linuxTab: 'systemConfig' },
  { labelKey: 'analysis.sidebar.linuxWebServices', icon: Globe, category: 'LinuxWebServices', linuxTab: 'webServices' },
  { labelKey: 'analysis.sidebar.linuxMysqlServices', icon: Database, category: 'LinuxMysqlServices', linuxTab: 'mysqlServices' },
  { labelKey: 'analysis.sidebar.deletedRecovery', icon: FileX2, linuxTab: 'deletedRecovery' },
];

export const ANDROID_NODES: SourceTreeNode[] = [
  { labelKey: 'analysis.android.deviceTab', icon: Smartphone, androidTab: 'device' },
  { labelKey: 'analysis.android.packagesTab', icon: AppWindow, androidTab: 'packages' },
];

