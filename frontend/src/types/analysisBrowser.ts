import type { AnalysisParseStatus } from './analysis';

export type BrowserKind = 'Chrome' | 'Edge' | 'Firefox' | string;

export interface BrowserHistorySummary {
  status: AnalysisParseStatus;
  visitTotal: number;
  downloadTotal: number;
  cookieTotal: number;
  sessionTotal: number;
  passwordTotal: number;
  visits: BrowserVisit[];
  downloads: BrowserDownload[];
  cookies: BrowserCookie[];
  sessions: BrowserSessionTab[];
  passwords: BrowserPassword[];
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

export interface BrowserCookie {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile?: string;
  domain: string;
  name: string;
  valuePreview?: string;
  expiry?: string;
  secure: boolean;
  httpOnly: boolean;
  sameSite?: number;
}

export interface BrowserSessionTab {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile?: string;
  url: string;
  title?: string;
  windowIndex: number;
  tabIndex: number;
  lastActive?: string;
}

export interface BrowserPassword {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile?: string;
  url: string;
  username: string;
  passwordPreview?: string;
  createdAt?: string;
  timesUsed: number;
}
