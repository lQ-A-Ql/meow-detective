import type { AnalysisParseStatus } from './analysis';

export type BrowserKind = 'Chrome' | 'Edge' | 'Firefox' | string;

export interface BrowserHistorySummary {
  status: AnalysisParseStatus;
  visitTotal: number;
  downloadTotal: number;
  visits: BrowserVisit[];
  downloads: BrowserDownload[];
  generatedAt: string;
  warnings: string[];
}

export interface BrowserVisit {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile: string;
  url: string;
  title: string;
  visitTime?: string;
  visitCount: number;
}

export interface BrowserDownload {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile: string;
  url: string;
  targetPath: string;
  startTime?: string;
  totalBytes: number;
}
