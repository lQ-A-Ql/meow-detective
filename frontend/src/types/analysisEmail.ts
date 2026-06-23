import type { AnalysisParseStatus } from './analysis';

export interface EmailExtractionSummary {
  status: AnalysisParseStatus;
  total: number;
  messages: EmailMessage[];
  generatedAt: string;
  warnings: string[];
}

export interface EmailMessage {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  sentAt?: string;
  from: string;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  messageId: string;
  attachments: string[];
  bodyPreview: string;
}
